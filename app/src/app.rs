//! Root component: hash routing between the home page and the gameplay
//! canvas. Real screens (song select, options, calibration, results) arrive in
//! phase 3.

use gloo::events::EventListener;
use yew::prelude::*;

use crate::components::calibrate::Calibrate;
use crate::components::credits::CreditsScreen;
use crate::components::device_watcher::DeviceWatcher;
use crate::components::game_canvas::{GameCanvas, SongSource};
use crate::components::options::Options;
use crate::components::skin_preview::SkinPreview;
use crate::components::song_select::SongSelect;
use crate::components::ui_dev::UiDev;
use crate::components::ui_test::UiTest;
use crate::components::video_check::VideoCheck;
use crate::router::Route;
use crate::web::gfx::BackendPreference;

pub(crate) enum Msg {
    RouteChanged,
    /// The app's device was lost and made again.
    DeviceLost,
    /// The play screen starts over (its device was lost).
    Remount,
}

pub(crate) struct App {
    route: Route,
    _hash_listener: Option<EventListener>,
    _lost: crate::web::screen::Listener,
    /// Bumped when the play screen asks to start over: part of its key.
    play_mounts: u32,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link().clone();
        let listener = web_sys::window().map(|w| {
            EventListener::new(&w, "hashchange", move |_| {
                link.send_message(Msg::RouteChanged)
            })
        });
        let link = ctx.link().clone();
        App {
            route: Route::current(),
            _hash_listener: listener,
            _lost: crate::web::screen::on_lost(move || link.send_message(Msg::DeviceLost)),
            play_mounts: 0,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            // The boot shell stays until the device is there (or known
            // not to be): the first paint has what it draws with.
            wasm_bindgen_futures::spawn_local(async {
                let _ = crate::web::screen::gfx().await;
                crate::boot::ready();
            });
            // Never longer than this, should the adapter not answer.
            gloo::timers::callback::Timeout::new(4000, crate::boot::ready).forget();
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            // A screen on the device starts over on the new one.
            Msg::DeviceLost => true,
            Msg::Remount => {
                self.play_mounts += 1;
                true
            }
            Msg::RouteChanged => {
                let route = Route::current();
                if route != self.route {
                    self.route = route;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let suppressed = matches!(
            self.route,
            Route::Play { .. }
                | Route::CalibrateRun { .. }
                | Route::Calibrate
                | Route::UiTest { .. }
                | Route::UiDev { .. }
        );
        let screen = self.screen(ctx);
        html! {
            <>
                <DeviceWatcher suppressed={suppressed} route={self.route.to_hash()} />
                { screen }
            </>
        }
    }
}

impl App {
    fn screen(&self, ctx: &Context<Self>) -> Html {
        match &self.route {
            Route::Home => html! {
                <main class="shell">
                    <h1>{ "Dito Dito Infinito" }<span class="demo-tag">{ "demo" }</span></h1>
                    <SongSelect />
                    <p class="footer-links muted">
                        <a href={Route::Options.to_hash()}>{ "options" }</a>
                        { " · " }
                        <a href={Route::Calibrate.to_hash()}>{ "calibrate" }</a>
                        { " · " }
                        <a href={Route::Credits.to_hash()}>{ "credits" }</a>
                        { " · " }
                        <a href="https://github.com/sugoijan/ditoditoinfinito">{ "source" }</a>
                    </p>
                </main>
            },
            Route::Play {
                song,
                chart,
                force_gl,
                auto,
                auto_pad,
                bias_ms,
                slow,
            } => html! {
                <GameCanvas
                    key={format!("{song}/{chart}/{}", self.play_mounts)}
                    restart={ctx.link().callback(|()| Msg::Remount)}
                    source={SongSource::Song { id: song.clone(), chart: *chart }}
                    auto={*auto}
                    auto_pad={*auto_pad}
                    auto_bias={*bias_ms as f64 / 1000.0}
                    slow={*slow}
                    backend={if *force_gl { BackendPreference::WebGl2 } else { BackendPreference::Auto }}
                />
            },
            Route::CalibrateRun {
                mode,
                auto,
                bias_ms,
            } => html! {
                <GameCanvas
                    key={format!("calibrate/{mode}/{}", self.play_mounts)}
                    restart={ctx.link().callback(|()| Msg::Remount)}
                    source={SongSource::Calibration(crate::calibration::CalMode::parse(mode))}
                    auto={*auto}
                    auto_bias={*bias_ms as f64 / 1000.0}
                />
            },
            Route::Credits => html! {
                <>
                    <CreditsScreen />
                    <p class="shell"><a href={Route::Home.to_hash()}>{ "← back" }</a></p>
                </>
            },
            Route::Options => html! { <Options /> },
            Route::VideoCheck { sources, decoder } => {
                html! { <VideoCheck sources={sources.clone()} decoder={decoder.clone()} /> }
            }
            Route::Calibrate => html! { <Calibrate /> },
            Route::SkinPreview { params } => html! { <SkinPreview params={params.clone()} /> },
            Route::UiTest { params } => html! {
                <UiTest key={crate::web::screen::losses()} params={params.clone()} />
            },
            Route::UiDev { screen } => html! {
                // Keyed by the screen too: a new `screen` in the address
                // starts the page over on that screen.
                <UiDev key={format!("{}-{screen}", crate::web::screen::losses())} screen={screen.clone()} />
            },
            Route::NotFound(p) => html! {
                <main class="shell">
                    <p>{ format!("no route `{p}`") }</p>
                    <a href={Route::Home.to_hash()}>{ "← home" }</a>
                </main>
            },
        }
    }
}
