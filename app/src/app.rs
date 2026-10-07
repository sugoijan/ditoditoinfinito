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
use crate::components::song_select::SongSelect;
use crate::router::Route;
use crate::web::gfx::BackendPreference;

pub(crate) enum Msg {
    RouteChanged,
}

pub(crate) struct App {
    route: Route,
    _hash_listener: Option<EventListener>,
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
        App {
            route: Route::current(),
            _hash_listener: listener,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            crate::boot::ready();
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
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

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let suppressed = matches!(
            self.route,
            Route::Play { .. } | Route::CalibrateRun { .. } | Route::Calibrate
        );
        let screen = self.screen();
        html! {
            <>
                <DeviceWatcher suppressed={suppressed} route={self.route.to_hash()} />
                { screen }
            </>
        }
    }
}

impl App {
    fn screen(&self) -> Html {
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
            } => html! {
                <GameCanvas
                    key={format!("{song}/{chart}")}
                    source={SongSource::Song { id: song.clone(), chart: *chart }}
                    auto={*auto}
                    auto_pad={*auto_pad}
                    auto_bias={*bias_ms as f64 / 1000.0}
                    backend={if *force_gl { BackendPreference::WebGl2 } else { BackendPreference::Auto }}
                />
            },
            Route::CalibrateRun {
                mode,
                auto,
                bias_ms,
            } => html! {
                <GameCanvas
                    key={format!("calibrate/{mode}")}
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
            Route::Calibrate => html! { <Calibrate /> },
            Route::NotFound(p) => html! {
                <main class="shell">
                    <p>{ format!("no route `{p}`") }</p>
                    <a href={Route::Home.to_hash()}>{ "← home" }</a>
                </main>
            },
        }
    }
}
