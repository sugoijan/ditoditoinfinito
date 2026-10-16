//! Watches the audio output path and the display across the whole app and
//! tells the player when the game meets a device it has no calibration for:
//! a banner drawn by our theme on every canvas page (`crate::notices`, the
//! UI's chrome: `ddi_ui::screens::notices`), whose wording and actions are
//! `ddi_ui::screens::notices::device_notice`'s.
//!
//! Audio cannot be probed before a user gesture, so: with no profiles at all
//! a "first time" banner shows immediately; the first click or key anywhere
//! starts a background audio context and the first probe; `devicechange`
//! (output device added/removed/switched) and display-scale changes trigger
//! re-probes. Banners are held back while a song is playing. "later" hides
//! the banner until a new device shows up; a change to known devices shows
//! a short line ("devices: …") for five seconds.

use std::rc::Rc;

use crate::component::{Component, Context};
use ddi_platform::DeviceProfile;
use gloo::events::EventListener;
use gloo::timers::callback::Timeout;

use crate::settings::Settings;
use crate::web::audio::WebAudio;
use crate::web::devices::{self, RefreshSampler};
use crate::web::display;

/// Whether banners are held back at first ([`Msg::Route`] after).
pub(crate) struct Props {
    pub(crate) suppressed: bool,
}

pub(crate) enum Msg {
    Gesture,
    StartSampling,
    RefreshMeasured(f64),
    DevicesChanged,
    /// Debounced follow-up to `DevicesChanged`.
    Reprobe,
    Probed(DeviceProfile),
    /// `Probed`, applied after a short delay so any click in flight on the
    /// banner that triggered the probe can finish first.
    Apply(DeviceProfile),
    /// The route changed: held back or not (true while a song or
    /// calibration is running, or on the calibration pages themselves;
    /// banners wait or are redundant there). The banner is evaluated
    /// against the settings again (a calibration may just have saved a
    /// profile).
    Route {
        suppressed: bool,
    },
    /// A banner's button (`defaults`, `copy-audio:<id>`,
    /// `copy-display:<id>`, `later`; `calibrate` navigates by itself).
    Action(String),
    HideToast,
}

pub(crate) struct DeviceWatcher {
    suppressed: bool,
    audio: Option<Rc<WebAudio>>,
    refresh_hz: f64,
    last: Option<DeviceProfile>,
    toast: Option<String>,
    dismissed: bool,
    /// The devices (audio and display ids) "later" was said for.
    dismissed_for: Option<(String, String)>,
    probing: bool,
    _gesture: Vec<EventListener>,
    _devicechange: Option<EventListener>,
    _dpr: Option<(web_sys::MediaQueryList, EventListener)>,
    _sampler: Option<RefreshSampler>,
    _debounce: Option<Timeout>,
    _toast_timer: Option<Timeout>,
    _apply_timer: Option<Timeout>,
}

impl Component for DeviceWatcher {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link();
        // `click` rather than `pointerdown`: by the time a click bubbles to
        // the document, a clicked link has already committed its navigation,
        // so the probe that follows (and the re-render it causes) cannot
        // race the click itself.
        let mut gesture = Vec::new();
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            for ev in ["click", "keydown"] {
                let l = link.clone();
                gesture.push(EventListener::new(&document, ev, move |_| {
                    l.send_message(Msg::Gesture)
                }));
            }
        }
        let devicechange = crate::web::devices::media_devices().map(|md| {
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
        {
            let l = link.clone();
            crate::notices::on_action(Some(Rc::new(move |id: &str| {
                l.send_message(Msg::Action(id.to_string()))
            })));
        }
        let watcher = DeviceWatcher {
            suppressed: ctx.props().suppressed,
            audio: None,
            refresh_hz: 60.0,
            last: None,
            toast: None,
            // "later", said earlier this session (a reload keeps it).
            dismissed: later::load().is_some(),
            dismissed_for: later::load().flatten(),
            probing: false,
            _gesture: gesture,
            _devicechange: devicechange,
            _dpr: dpr,
            _sampler: None,
            _debounce: Some(sampler_timer),
            _toast_timer: None,
            _apply_timer: None,
        };
        watcher.publish();
        watcher
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Gesture => {
                if let Some(a) = &self.audio {
                    // Suspended during a play and not resumed after it
                    // (a browser that wants a gesture for that): now.
                    if !self.suppressed && !a.running() {
                        a.resume_now();
                    }
                    return false;
                }
                // Inside the gesture: the context may start.
                if let Ok(a) = WebAudio::new() {
                    self.audio = Some(Rc::new(a));
                    self.probe(ctx);
                }
            }
            Msg::StartSampling => {
                let l = ctx.link().clone();
                self._sampler = Some(devices::sample_refresh_rate(120, move |hz| {
                    l.send_message(Msg::RefreshMeasured(hz))
                }));
            }
            Msg::RefreshMeasured(hz) => {
                self.refresh_hz = hz;
                self._sampler = None;
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
            }
            Msg::Reprobe => {
                self.probe(ctx);
            }
            Msg::Probed(profile) => {
                self.probing = false;
                // Probed during a play (its first gesture made the
                // context): suspended until the play is over.
                if self.suppressed
                    && let Some(a) = &self.audio
                {
                    a.suspend_now();
                }
                let link = ctx.link().clone();
                self._apply_timer = Some(Timeout::new(400, move || {
                    link.send_message(Msg::Apply(profile))
                }));
            }
            Msg::Apply(profile) => {
                let settings = Settings::load();
                let changed = self.last.as_ref() != Some(&profile);
                self.last = Some(profile.clone());
                let unknown =
                    ddi_ui::screens::notices::device_notice(&settings, Some(&profile)).is_some();
                // "later" holds for the devices it was said on (those
                // probed then, or, said before any probe, the first ones
                // probed); new ones bring the banner back.
                let ids = (profile.audio.id.clone(), profile.display.id.clone());
                if unknown {
                    if self.dismissed {
                        match &self.dismissed_for {
                            None => {
                                self.dismissed_for = Some(ids);
                                later::save(self.dismissed_for.clone());
                            }
                            Some(d) if *d == ids => {}
                            Some(_) => {
                                self.dismissed = false;
                                self.dismissed_for = None;
                                later::clear();
                            }
                        }
                    }
                } else if changed {
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
            Msg::Action(id) => match id.as_str() {
                "later" => {
                    self.dismissed = true;
                    self.dismissed_for = self
                        .last
                        .as_ref()
                        .map(|p| (p.audio.id.clone(), p.display.id.clone()));
                    later::save(self.dismissed_for.clone());
                }
                "calibrate" => {}
                other => {
                    if let Some(profile) = self.last.clone() {
                        let mut settings = Settings::load();
                        ddi_ui::screens::notices::register_devices(&mut settings, &profile, other);
                        settings.save();
                    }
                }
            },
            Msg::HideToast => self.toast = None,
            Msg::Route { suppressed } => {
                // A play (or calibration) has its own context: this one
                // is suspended meanwhile (no second live output), and on
                // again after.
                if suppressed != self.suppressed
                    && let Some(a) = &self.audio
                {
                    if suppressed {
                        a.suspend_now();
                    } else {
                        a.resume_now();
                    }
                }
                self.suppressed = suppressed;
            }
        }
        self.publish();
        false
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        crate::notices::on_action(None);
        crate::notices::set(Default::default());
    }
}

impl DeviceWatcher {
    /// What the pages show: the banner (unless held back or put off) and
    /// the short line.
    fn publish(&self) {
        let suppressed = self.suppressed;
        let banner = if suppressed || self.dismissed {
            None
        } else {
            ddi_ui::screens::notices::device_notice(&Settings::load(), self.last.as_ref())
        };
        crate::notices::set(ddi_ui::screens::notices::AppNotices {
            banner,
            toast: if suppressed { None } else { self.toast.clone() },
        });
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
}

/// "later" for the session (`sessionStorage`): the devices it was said
/// for (none yet: said before the first probe).
mod later {
    use ddi_platform::storage::{self, Area};

    const KEY: &str = "ddi.notices.later.v1";

    pub(super) fn load() -> Option<Option<(String, String)>> {
        storage::load(Area::Session, KEY)
    }

    pub(super) fn save(devices: Option<(String, String)>) {
        let _ = storage::save(Area::Session, KEY, &devices);
    }

    pub(super) fn clear() {
        storage::remove(Area::Session, KEY);
    }
}
