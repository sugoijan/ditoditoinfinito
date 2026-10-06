//! Watches the audio output path and the display across the whole app and
//! shows a banner when the game meets a device it has no calibration for.
//!
//! Audio cannot be probed before a user gesture, so: with no profiles at all
//! a "first time" banner shows immediately; the first click or key anywhere
//! starts a background audio context and the first probe; `devicechange`
//! (output device added/removed/switched) and display-scale changes trigger
//! re-probes. Banners are held back while a song is playing.

use std::rc::Rc;

use ddi_platform::DeviceProfile;
use gloo::events::{EventListener, EventListenerOptions};
use gloo::timers::callback::Timeout;
use yew::prelude::*;

use crate::router::Route;
use crate::settings::Settings;
use crate::web::audio::WebAudio;
use crate::web::devices::{self, RefreshSampler};
use crate::web::display;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// True while a song or calibration is running, or on the calibration
    /// pages themselves; banners wait or are redundant there.
    pub(crate) suppressed: bool,
    /// Current route; a change re-evaluates the banner against the settings
    /// (a calibration may just have saved a profile).
    pub(crate) route: String,
}

pub(crate) enum Msg {
    Gesture,
    StartSampling,
    RefreshMeasured(f64),
    DevicesChanged,
    /// Debounced follow-up to `DevicesChanged`.
    Reprobe,
    Probed(DeviceProfile),
    UseDefaults,
    CopyAudioFrom(String),
    CopyDisplayFrom(String),
    Dismiss,
    HideToast,
}

enum Banner {
    /// No profiles at all yet.
    FirstRun,
    /// A probed profile with at least one unknown device.
    Unknown {
        profile: DeviceProfile,
        new_audio: bool,
        new_display: bool,
    },
}

pub(crate) struct DeviceWatcher {
    audio: Option<Rc<WebAudio>>,
    refresh_hz: f64,
    last: Option<DeviceProfile>,
    banner: Option<Banner>,
    toast: Option<String>,
    dismissed: bool,
    probing: bool,
    _gesture: Vec<EventListener>,
    _devicechange: Option<EventListener>,
    _dpr: Option<(web_sys::MediaQueryList, EventListener)>,
    _sampler: Option<RefreshSampler>,
    _debounce: Option<Timeout>,
    _toast_timer: Option<Timeout>,
}

impl Component for DeviceWatcher {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link();
        let mut gesture = Vec::new();
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            for ev in ["pointerdown", "keydown"] {
                let l = link.clone();
                gesture.push(EventListener::new_with_options(
                    &document,
                    ev,
                    EventListenerOptions::run_in_capture_phase(),
                    move |_| l.send_message(Msg::Gesture),
                ));
            }
        }
        let devicechange = web_sys::window()
            .and_then(|w| w.navigator().media_devices().ok())
            .map(|md| {
                let l = link.clone();
                EventListener::new(&md, "devicechange", move |_| {
                    l.send_message(Msg::DevicesChanged)
                })
            });
        let dpr = {
            let l = link.clone();
            display::dpr_change_listener(move || l.send_message(Msg::DevicesChanged))
        };
        // Start sampling once start-up work (wasm init, first layout) is over.
        let sampler_timer = {
            let l = link.clone();
            Timeout::new(1500, move || l.send_message(Msg::StartSampling))
        };
        let settings = Settings::load();
        let banner = (settings.audio_profiles.is_empty() && settings.display_profiles.is_empty())
            .then_some(Banner::FirstRun);
        DeviceWatcher {
            audio: None,
            refresh_hz: 60.0,
            last: None,
            banner,
            toast: None,
            dismissed: false,
            probing: false,
            _gesture: gesture,
            _devicechange: devicechange,
            _dpr: dpr,
            _sampler: None,
            _debounce: Some(sampler_timer),
            _toast_timer: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Gesture => {
                if self.audio.is_some() {
                    return false;
                }
                // Inside the gesture: the context may start.
                if let Ok(a) = WebAudio::new() {
                    self.audio = Some(Rc::new(a));
                    self._gesture.clear();
                    self.probe(ctx);
                }
                false
            }
            Msg::StartSampling => {
                let l = ctx.link().clone();
                self._sampler = Some(devices::sample_refresh_rate(120, move |hz| {
                    l.send_message(Msg::RefreshMeasured(hz))
                }));
                false
            }
            Msg::RefreshMeasured(hz) => {
                self.refresh_hz = hz;
                self._sampler = None;
                false
            }
            Msg::DevicesChanged => {
                // The DPR query only watches the old value; re-arm it.
                let l = ctx.link().clone();
                self._dpr =
                    display::dpr_change_listener(move || l.send_message(Msg::DevicesChanged));
                let l = ctx.link().clone();
                self._sampler = Some(devices::sample_refresh_rate(90, move |hz| {
                    l.send_message(Msg::RefreshMeasured(hz))
                }));
                // Let the browser settle on the new device, then probe. Without
                // a gesture yet there is no audio context; the first gesture
                // will probe anyway.
                let link = ctx.link().clone();
                self._debounce = Some(Timeout::new(1800, move || link.send_message(Msg::Reprobe)));
                false
            }
            Msg::Reprobe => {
                self.probe(ctx);
                false
            }
            Msg::Probed(profile) => {
                self.probing = false;
                let settings = Settings::load();
                let changed = self.last.as_ref() != Some(&profile);
                self.last = Some(profile.clone());
                let was_unknown = self.reevaluate(&settings);
                if was_unknown {
                    self.dismissed = false;
                } else {
                    if changed {
                        let a = settings.audio_profile(&profile.audio.id);
                        let d = settings.display_profile(&profile.display.id);
                        self.toast = Some(format!(
                            "devices: {} ({:+.0} ms) · {} ({:+.0} ms)",
                            profile.audio.label,
                            a.map(|p| p.offset).unwrap_or(0.0) * 1000.0,
                            profile.display.label,
                            d.map(|p| p.offset).unwrap_or(0.0) * 1000.0
                        ));
                        let link = ctx.link().clone();
                        self._toast_timer = Some(Timeout::new(5000, move || {
                            link.send_message(Msg::HideToast)
                        }));
                    }
                }
                true
            }
            Msg::UseDefaults => {
                self.register(None, None);
                true
            }
            Msg::CopyAudioFrom(id) => {
                self.register(Some(&id), None);
                true
            }
            Msg::CopyDisplayFrom(id) => {
                self.register(None, Some(&id));
                true
            }
            Msg::Dismiss => {
                self.dismissed = true;
                true
            }
            Msg::HideToast => {
                self.toast = None;
                true
            }
        }
    }

    fn changed(&mut self, _ctx: &Context<Self>, _old: &Self::Properties) -> bool {
        // Route changed: settings may have gained profiles (calibration).
        let settings = Settings::load();
        self.reevaluate(&settings);
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        if ctx.props().suppressed {
            return html! {};
        }
        let link = ctx.link();
        let toast = self
            .toast
            .as_ref()
            .map(|t| html! { <div class="device-toast muted">{ t }</div> });
        if self.dismissed {
            return html! { { for toast } };
        }
        let settings = Settings::load();
        let banner = match &self.banner {
            None => html! {},
            Some(Banner::FirstRun) => html! {
                <div class="device-banner">
                    <span>{ "First time here? Calibrate the timing for your audio and display before playing; it takes a minute." }</span>
                    <a class="button" href={Route::Calibrate.to_hash()}>{ "calibrate" }</a>
                    <button class="link-button" onclick={link.callback(|_| Msg::Dismiss)}>{ "later" }</button>
                </div>
            },
            Some(Banner::Unknown {
                profile,
                new_audio,
                new_display,
            }) => {
                let copies = |kind: &'static str,
                              profiles: &[crate::settings::OffsetProfile]|
                 -> Html {
                    html! {
                        { for profiles.iter().filter(|p| p.calibrated).map(|p| {
                            let id = p.id.clone();
                            let onclick = if kind == "audio" {
                                link.callback(move |_| Msg::CopyAudioFrom(id.clone()))
                            } else {
                                link.callback(move |_| Msg::CopyDisplayFrom(id.clone()))
                            };
                            html!{ <button class="link-button" {onclick}>{ format!("use {:+.0} ms from {}", p.offset * 1000.0, p.label) }</button> }
                        }) }
                    }
                };
                html! {
                    <div class="device-banner">
                        <span>
                            { if *new_audio { format!("New audio path: {}. ", profile.audio.label) } else { String::new() } }
                            { if *new_display { format!("New display: {}. ", profile.display.label) } else { String::new() } }
                            { "No calibration for it yet." }
                        </span>
                        <a class="button" href={Route::Calibrate.to_hash()}>{ "calibrate" }</a>
                        <button class="link-button" onclick={link.callback(|_| Msg::UseDefaults)}>{ "use defaults" }</button>
                        { if *new_audio { copies("audio", &settings.audio_profiles) } else { html!{} } }
                        { if *new_display { copies("display", &settings.display_profiles) } else { html!{} } }
                        <button class="link-button" onclick={link.callback(|_| Msg::Dismiss)}>{ "later" }</button>
                    </div>
                }
            }
        };
        html! { <>{ banner }{ for toast }</> }
    }
}

impl DeviceWatcher {
    /// Recomputes the banner from the last probe and the stored profiles.
    /// Returns whether the current devices are (still) unknown.
    fn reevaluate(&mut self, settings: &Settings) -> bool {
        match &self.last {
            Some(profile) => {
                let new_audio = settings.audio_profile(&profile.audio.id).is_none();
                let new_display = settings.display_profile(&profile.display.id).is_none();
                if new_audio || new_display {
                    self.banner = Some(Banner::Unknown {
                        profile: profile.clone(),
                        new_audio,
                        new_display,
                    });
                    true
                } else {
                    self.banner = None;
                    false
                }
            }
            None => {
                let first =
                    settings.audio_profiles.is_empty() && settings.display_profiles.is_empty();
                self.banner = first.then_some(Banner::FirstRun);
                first
            }
        }
    }

    fn probe(&mut self, ctx: &Context<Self>) {
        let Some(audio) = self.audio.clone() else {
            return;
        };
        if self.probing {
            return;
        }
        self.probing = true;
        let hz = self.refresh_hz;
        ctx.link().send_future(async move {
            let profile = devices::probe(&audio, hz).await;
            Msg::Probed(profile)
        });
    }

    fn register(&mut self, copy_audio_from: Option<&str>, copy_display_from: Option<&str>) {
        let Some(Banner::Unknown { profile, .. }) = &self.banner else {
            return;
        };
        let mut settings = Settings::load();
        let audio_initial = copy_audio_from
            .and_then(|id| settings.audio_profile(id).map(|p| p.offset))
            .unwrap_or(settings.audio_offset);
        let display_initial = copy_display_from
            .and_then(|id| settings.display_profile(id).map(|p| p.offset))
            .unwrap_or(settings.visual_offset);
        Settings::touch_profile(&mut settings.audio_profiles, &profile.audio, audio_initial);
        Settings::touch_profile(
            &mut settings.display_profiles,
            &profile.display,
            display_initial,
        );
        settings.save();
        self.banner = None;
    }
}
