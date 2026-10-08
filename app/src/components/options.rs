//! Options screen: speed, scroll, note options, ruleset, offsets, volume
//! (with a sample to judge it by), display, controls, and resetting
//! settings or the imported songs.
//! Settings are saved to `localStorage` on every change.
//!
//! Controls list the keyboard, every connected controller and every
//! controller with saved bindings. Controllers are found by polling (there
//! is no reliable connect event, and some browsers only reveal a pad once
//! a button is pressed), so the list is refreshed on a timer while the
//! screen is open. Binding itself is the guided flow in [`BindFlow`].

use ddi_engine::{Appearance, ScrollAction, TimingCut, Turn};
use ddi_platform::DeviceId;
use ddi_platform::gamepad::Control;
use gloo::events::EventListener;
use gloo::timers::callback::Interval;
use wasm_bindgen::JsCast;
use web_sys::{AudioBuffer, HtmlInputElement, HtmlSelectElement, KeyboardEvent};
use yew::prelude::*;

use crate::components::bind_flow::{BindFlow, Bound};
use crate::import;
use crate::preview::Preview;
use crate::router::Route;
use crate::settings::{FIELD_FILTERS, PadBindings, Settings, slider_to_volume, volume_to_slider};
use crate::songs::{Library, ManifestEntry, clear_imported, is_imported_id};
use crate::web::gamepad::Gamepads;

/// How often the controller list is compared with the connected pads.
const PAD_REFRESH_MS: u32 = 300;

pub(crate) enum Msg {
    Speed(f64),
    Reverse(bool),
    ScrollAction(ScrollAction),
    Turn(Turn),
    Appearance(Appearance),
    Cut(TimingCut),
    NoJumps(bool),
    NoHolds(bool),
    Ruleset(String),
    AudioOffsetMs(f64),
    VisualOffsetMs(f64),
    Volume(f32),
    Debug(bool),
    ShowDeltas(bool),
    ReceptorSnap(bool),
    /// Background brightness, 0..=1.
    BgBrightness(f32),
    FieldFilter(f32),
    /// Edit a profile's offset (kind "audio"/"display", id, ms).
    ProfileOffsetMs(&'static str, String, f64),
    DeleteProfile(&'static str, String),
    /// Start listening for a key to bind to `lane`.
    Capture(usize),
    Captured(String),
    ClearLane(usize),
    ResetKeys,
    /// Open the guided binding flow, for a device or for the next one that
    /// produces a press.
    Bind(Option<DeviceId>),
    BindSaved(Bound),
    BindClosed,
    /// Timer: compare the connected controllers with the list shown.
    PadsTick,
    ClearPad(String),
    ForgetPad(String),
    FastPadPoll(bool),
    /// The song list loaded: a bundled song for the volume sample and the
    /// number of imported songs.
    Library(Option<ManifestEntry>, usize),
    /// Volume sample: started in the click (see [`Preview::start`]).
    SampleStarted(Preview),
    SampleDecoded(AudioBuffer),
    SampleEnded,
    /// First click on a reset button: ask for a second one.
    AskReset(Reset),
    Reset(Reset),
    ResetDone(Reset, Result<(), String>),
}

/// What a reset button clears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reset {
    /// Every setting (including device calibrations) except song offsets.
    Settings,
    /// Imported songs, their files and their offsets.
    Songs,
    /// Both, and the offsets of the bundled songs too.
    Everything,
}

pub(crate) struct Options {
    settings: Settings,
    capturing: Option<usize>,
    /// Bundled song whose preview serves as the volume sample.
    sample_song: Option<ManifestEntry>,
    sample: Option<Preview>,
    imported: usize,
    confirm: Option<Reset>,
    /// Result of the last reset.
    notice: Option<String>,
    gamepads: Option<Gamepads>,
    /// Connected controllers `(id, standard mapping, count)`; identical
    /// controllers share an id and their bindings.
    connected: Vec<(String, bool, usize)>,
    /// The guided flow, when open: `Some(None)` waits for any device.
    binding: Option<Option<DeviceId>>,
    _keys: Option<EventListener>,
    _pad_refresh: Option<Interval>,
}

const LANE_NAMES: [&str; 4] = ["Left", "Down", "Up", "Right"];

impl Component for Options {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link().clone();
        let keys = web_sys::window().map(|w| {
            EventListener::new(&w, "keydown", move |event| {
                if let Some(e) = event.dyn_ref::<KeyboardEvent>() {
                    link.send_message(Msg::Captured(e.code()));
                }
            })
        });
        ctx.link().send_future(async {
            match Library::load().await {
                Ok(l) => Msg::Library(l.bundled.into_iter().next(), l.imported.len()),
                Err(_) => Msg::Library(None, 0),
            }
        });
        let gamepads = Gamepads::new();
        let pad_refresh = gamepads.as_ref().map(|_| {
            let link = ctx.link().clone();
            Interval::new(PAD_REFRESH_MS, move || link.send_message(Msg::PadsTick))
        });
        Options {
            settings: Settings::load(),
            capturing: None,
            sample_song: None,
            sample: None,
            imported: 0,
            confirm: None,
            notice: None,
            gamepads,
            connected: Vec::new(),
            binding: None,
            _keys: keys,
            _pad_refresh: pad_refresh,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Library(song, imported) => {
                self.sample_song = song;
                self.imported = imported;
                return true;
            }
            Msg::SampleStarted(preview) => {
                let load = preview.load();
                ctx.link().send_future(async move {
                    match load.await {
                        Ok(buffer) => Msg::SampleDecoded(buffer),
                        Err(e) => {
                            web_sys::console::warn_1(&format!("volume sample: {e}").into());
                            Msg::SampleEnded
                        }
                    }
                });
                self.sample = Some(preview);
                return true;
            }
            Msg::SampleDecoded(buffer) => {
                let volume = self.settings.volume;
                let link = ctx.link().clone();
                if let Some(p) = &mut self.sample
                    && p.play(&buffer, volume, move || link.send_message(Msg::SampleEnded))
                        .is_err()
                {
                    self.sample = None;
                    return true;
                }
                return false;
            }
            Msg::SampleEnded => {
                self.sample = None;
                return true;
            }
            Msg::AskReset(r) => {
                self.confirm = Some(r);
                self.notice = None;
                return true;
            }
            Msg::Reset(r) => {
                self.confirm = None;
                if r != Reset::Songs {
                    // Song offsets belong to the songs, not to the settings.
                    let offsets = std::mem::take(&mut self.settings.song_offsets);
                    self.settings = Settings::default();
                    if r == Reset::Settings {
                        self.settings.song_offsets = offsets;
                    }
                }
                if r == Reset::Songs {
                    self.settings
                        .song_offsets
                        .retain(|id, _| !is_imported_id(id));
                }
                self.settings.save();
                if let Some(p) = &self.sample {
                    p.set_volume(self.settings.volume);
                }
                if r == Reset::Settings {
                    self.notice = Some("Settings reset.".into());
                } else {
                    ctx.link()
                        .send_future(async move { Msg::ResetDone(r, clear_imported().await) });
                }
                return true;
            }
            Msg::ResetDone(r, result) => {
                self.notice = Some(match (r, result) {
                    (_, Err(e)) => format!("Could not remove the imported songs: {e}"),
                    (Reset::Everything, Ok(())) => {
                        "Settings reset and imported songs removed.".into()
                    }
                    (_, Ok(())) => "Imported songs removed.".into(),
                });
                self.imported = 0;
                return true;
            }
            Msg::Speed(v) => self.settings.speed = v.clamp(0.25, 10.0),
            Msg::Reverse(v) => self.settings.reverse = v,
            Msg::ScrollAction(v) => self.settings.scroll_action = v,
            Msg::Turn(v) => self.settings.transform.turn = v,
            Msg::Appearance(v) => self.settings.appearance = v,
            Msg::Cut(v) => self.settings.transform.cut = v,
            Msg::NoJumps(v) => self.settings.transform.no_jumps = v,
            Msg::NoHolds(v) => self.settings.transform.no_holds = v,
            Msg::Ruleset(id) => self.settings.ruleset = id,
            Msg::AudioOffsetMs(ms) => self.settings.audio_offset = (ms / 1000.0).clamp(-0.5, 0.5),
            Msg::VisualOffsetMs(ms) => self.settings.visual_offset = (ms / 1000.0).clamp(-0.5, 0.5),
            Msg::Volume(v) => {
                self.settings.volume = v.clamp(0.0, 1.0);
                if let Some(p) = &self.sample {
                    p.set_volume(self.settings.volume);
                }
            }
            Msg::Debug(v) => self.settings.debug = v,
            Msg::ShowDeltas(v) => self.settings.show_deltas = v,
            Msg::ReceptorSnap(v) => self.settings.receptor_snap = v,
            Msg::BgBrightness(v) => self.settings.bg_brightness = v.clamp(0.0, 1.0),
            Msg::FieldFilter(v) => self.settings.field_filter = v,
            Msg::ProfileOffsetMs(kind, id, ms) => {
                let list = if kind == "audio" {
                    &mut self.settings.audio_profiles
                } else {
                    &mut self.settings.display_profiles
                };
                if let Some(p) = list.iter_mut().find(|p| p.id == id) {
                    p.offset = (ms / 1000.0).clamp(-0.5, 0.5);
                    p.calibrated = true;
                }
            }
            Msg::DeleteProfile(kind, id) => {
                let list = if kind == "audio" {
                    &mut self.settings.audio_profiles
                } else {
                    &mut self.settings.display_profiles
                };
                list.retain(|p| p.id != id);
            }
            Msg::Capture(lane) => {
                self.capturing = Some(lane);
                return true;
            }
            Msg::Captured(code) => {
                let Some(lane) = self.capturing.take() else {
                    return false;
                };
                if code == "Escape" {
                    return true;
                }
                // A key belongs to one lane only.
                for keys in self.settings.keys_single.iter_mut() {
                    keys.retain(|k| k != &code);
                }
                if let Some(keys) = self.settings.keys_single.get_mut(lane) {
                    keys.push(code);
                }
            }
            Msg::ClearLane(lane) => {
                if let Some(keys) = self.settings.keys_single.get_mut(lane) {
                    keys.clear();
                }
            }
            Msg::ResetKeys => self.settings.keys_single = Settings::default().keys_single,
            Msg::Bind(device) => {
                self.capturing = None;
                self.binding = Some(device);
                return true;
            }
            Msg::BindClosed => {
                self.binding = None;
                return true;
            }
            Msg::BindSaved(bound) => {
                self.binding = None;
                match bound {
                    Bound::Keyboard(keys) => self.settings.keys_single = keys,
                    Bound::Pad(pad) => self.settings.set_pad_bindings(pad),
                }
            }
            Msg::PadsTick => {
                let Some(g) = &self.gamepads else {
                    return false;
                };
                // Nothing here consumes the hub's edge queue.
                g.clear();
                let mut connected: Vec<(String, bool, usize)> = Vec::new();
                for p in g.pads() {
                    match connected.iter_mut().find(|(id, _, _)| *id == p.id) {
                        Some(c) => c.2 += 1,
                        None => connected.push((p.id, p.standard, 1)),
                    }
                }
                if connected == self.connected {
                    return false;
                }
                self.connected = connected;
                return true;
            }
            Msg::ClearPad(id) => self.settings.set_pad_bindings(PadBindings {
                id,
                ..PadBindings::default()
            }),
            Msg::ForgetPad(id) => self.settings.forget_pad(&id),
            Msg::FastPadPoll(v) => self.settings.fast_pad_poll = v,
        }
        self.settings.save();
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let s = &self.settings;
        let number = |v: f64, step: &str, min: &str, max: &str, on: Callback<f64>| {
            let oninput = Callback::from(move |e: InputEvent| {
                if let Some(input) = e.target_dyn_into::<HtmlInputElement>()
                    && let Ok(v) = input.value().parse::<f64>()
                {
                    on.emit(v);
                }
            });
            html! { <input type="number" value={format!("{v}")} step={step.to_string()} min={min.to_string()} max={max.to_string()} {oninput} /> }
        };
        let rulesets = ddi_engine::rules::presets::all();
        let onruleset = link.callback(|e: Event| {
            Msg::Ruleset(
                e.target_dyn_into::<HtmlSelectElement>()
                    .map(|s| s.value())
                    .unwrap_or_default(),
            )
        });
        html! {
            <main class="shell options">
                <h1>{ "Options" }</h1>
                <section>
                    <h2>{ "Scroll" }</h2>
                    <label>{ "Speed (×BPM) " }{ number(s.speed, "0.25", "0.25", "10", link.callback(Msg::Speed)) }</label>
                    <label>
                        <input type="checkbox" checked={s.reverse} onchange={link.callback(|e: Event| Msg::Reverse(e.target_dyn_into::<HtmlInputElement>().is_some_and(|i| i.checked())))} />
                        { " Reverse (receptors at the bottom)" }
                    </label>
                    <label>{ "Scroll action " }{ choice(s.scroll_action, &SCROLL_ACTIONS, link.callback(Msg::ScrollAction)) }</label>
                </section>
                { self.note_options_section(link) }
                <section>
                    <h2>{ "Rules" }</h2>
                    <label>{ "Ruleset " }
                        <select onchange={onruleset}>
                            { for rulesets.iter().map(|r| html!{
                                <option value={r.id.clone()} selected={r.id == s.ruleset}>
                                    { format!("{}{}", r.name, if r.approximate { " (approximate)" } else { "" }) }
                                </option>
                            }) }
                        </select>
                    </label>
                </section>
                <section>
                    <h2>{ "Timing" }</h2>
                    <p class="muted">{ "Use " }<a href={Route::Calibrate.to_hash()}>{ "calibration" }</a>{ " to measure these. Positive audio offset: you hear the music later than the clock says (typical for Bluetooth). Positive visual offset: arrows are drawn as if the song were slightly ahead." }</p>
                    <label>{ "Default audio offset (ms), for devices without a profile " }{ number((s.audio_offset * 1000.0).round(), "1", "-500", "500", link.callback(Msg::AudioOffsetMs)) }</label>
                    <label>{ "Default visual offset (ms), for displays without a profile " }{ number((s.visual_offset * 1000.0).round(), "1", "-500", "500", link.callback(Msg::VisualOffsetMs)) }</label>
                    <h3>{ "Known devices" }</h3>
                    <p class="muted">{ "Offsets are kept per audio path and per display; the game picks the matching one when a song starts and asks to calibrate when it meets a new device." }</p>
                    { profile_table("audio", &s.audio_profiles, link) }
                    { profile_table("display", &s.display_profiles, link) }
                </section>
                <section>
                    <h2>{ "Sound" }</h2>
                    { self.volume_control(link) }
                </section>
                <section>
                    <h2>{ "Display" }</h2>
                    <label>
                        <input type="checkbox" checked={s.show_deltas} onchange={link.callback(|e: Event| Msg::ShowDeltas(e.target_dyn_into::<HtmlInputElement>().is_some_and(|i| i.checked())))} />
                        { " Show timing error of each hit (ms early/late)" }
                    </label>
                    <label>
                        <input type="checkbox" checked={s.receptor_snap} onchange={link.callback(|e: Event| Msg::ReceptorSnap(e.target_dyn_into::<HtmlInputElement>().is_some_and(|i| i.checked())))} />
                        { " Receptor snap (draw a note exactly on the receptor in its closest frame)" }
                    </label>
                    <label>
                        <input type="checkbox" checked={s.debug} onchange={link.callback(|e: Event| Msg::Debug(e.target_dyn_into::<HtmlInputElement>().is_some_and(|i| i.checked())))} />
                        { " Debug overlay (backend, FPS, clock drift, error stats)" }
                    </label>
                    { self.background_control(link) }
                    <label>{ "Field filter " }{ choice(s.field_filter, &FIELD_FILTER_CHOICES, link.callback(Msg::FieldFilter)) }</label>
                    <p class="muted">{ "The field filter dims the area behind the lanes." }</p>
                    <p class="muted">
                        { "Diagnostics: " }
                        <a href={Route::Play { song: "some-things-must".into(), chart: 2, force_gl: false, auto: true, auto_pad: false, bias_ms: 0 }.to_hash()}>{ "autoplay demo" }</a>
                        { " · " }
                        <a href={Route::Play { song: "some-things-must".into(), chart: 2, force_gl: true, auto: true, auto_pad: false, bias_ms: 0 }.to_hash()}>{ "autoplay demo on WebGL2" }</a>
                        { " (forces the fallback renderer)" }
                    </p>
                </section>
                { self.controls_section(link) }
                { self.reset_section(link) }
                <p><a href={Route::Home.to_hash()}>{ "← back" }</a></p>
                { for self.binding.clone().map(|device| html! {
                    <BindFlow
                        {device}
                        gamepads={self.gamepads.clone()}
                        on_save={link.callback(Msg::BindSaved)}
                        on_cancel={link.callback(|_| Msg::BindClosed)}
                    />
                }) }
            </main>
        }
    }
}

impl Options {
    fn note_options_section(&self, link: &html::Scope<Self>) -> Html {
        let t = &self.settings.transform;
        let checkbox = |checked: bool, msg: fn(bool) -> Msg, text: &str| {
            let onchange = link.callback(move |e: Event| {
                msg(e
                    .target_dyn_into::<HtmlInputElement>()
                    .is_some_and(|i| i.checked()))
            });
            html! {
                <label>
                    <input type="checkbox" {checked} {onchange} />
                    { format!(" {text}") }
                </label>
            }
        };
        html! {
            <section>
                <h2>{ "Note options" }</h2>
                <label>{ "Turn " }{ choice(t.turn, &TURNS, link.callback(Msg::Turn)) }</label>
                <label>{ "Appearance " }{ choice(self.settings.appearance, &APPEARANCES, link.callback(Msg::Appearance)) }</label>
                <h3>{ "Simplify" }</h3>
                <label>{ "Notes " }{ choice(t.cut, &CUTS, link.callback(Msg::Cut)) }</label>
                { checkbox(t.no_jumps, Msg::NoJumps, "No jumps (one panel at a time)") }
                { checkbox(t.no_holds, Msg::NoHolds, "No holds (holds become taps)") }
                <p class="muted">{ "Simplified plays are marked as assisted on the results." }</p>
            </section>
        }
    }

    fn background_control(&self, link: &html::Scope<Self>) -> Html {
        let percent = (self.settings.bg_brightness * 100.0).round() as i32;
        let oninput = link.callback(|e: InputEvent| {
            let v = e
                .target_dyn_into::<HtmlInputElement>()
                .and_then(|i| i.value().parse::<f32>().ok())
                .unwrap_or(0.0);
            Msg::BgBrightness(v / 100.0)
        });
        html! {
            <div class="slider-control">
                <label for="bg-brightness">{ "Background brightness" }</label>
                <input id="bg-brightness" type="range" min="0" max="100" step="5" value={percent.to_string()} {oninput} />
                <span class="slider-value">{ if percent == 0 { "off".to_string() } else { format!("{percent}%") } }</span>
            </div>
        }
    }

    fn controls_section(&self, link: &html::Scope<Self>) -> Html {
        let s = &self.settings;
        let keyboard_status = if s.keys_single == Settings::default().keys_single {
            "defaults"
        } else {
            "saved"
        };
        // Connected controllers first, then saved ones not connected now,
        // most recently used first.
        let mut pads: Vec<(String, bool, usize)> = self.connected.clone();
        let mut saved: Vec<&PadBindings> = s
            .pads
            .iter()
            .filter(|p| !self.connected.iter().any(|(id, _, _)| *id == p.id))
            .collect();
        saved.sort_by(|a, b| b.last_seen.total_cmp(&a.last_seen));
        pads.extend(saved.into_iter().map(|p| (p.id.clone(), false, 0)));
        let fast = link.callback(|e: Event| {
            Msg::FastPadPoll(
                e.target_dyn_into::<HtmlInputElement>()
                    .is_some_and(|i| i.checked()),
            )
        });
        html! {
            <section>
                <h2>{ "Controls" }</h2>
                <p class="muted">
                    { "Each device keeps its own bindings. “bind arrows” asks you to press each arrow in turn. Controllers show up here once one of their buttons has been pressed." }
                </p>
                <p>
                    <button onclick={link.callback(|_| Msg::Bind(None))}>{ "bind any device…" }</button>
                </p>
                <div class="device-block">
                    <div class="device-head">
                        <strong>{ "Keyboard" }</strong>
                        <span class="bind-status">{ keyboard_status }</span>
                    </div>
                    <table class="keys-table">
                        { for (0..4).map(|lane| {
                            let keys = s.keys_single.get(lane).cloned().unwrap_or_default();
                            let capturing = self.capturing == Some(lane);
                            html! {
                                <tr>
                                    <td>{ LANE_NAMES[lane] }</td>
                                    <td><code>{ if keys.is_empty() { "—".to_string() } else { keys.join(", ") } }</code></td>
                                    <td>
                                        <button onclick={link.callback(move |_| Msg::Capture(lane))} disabled={self.capturing.is_some()}>
                                            { if capturing { "press a key…" } else { "add key" } }
                                        </button>
                                        { " " }
                                        <button onclick={link.callback(move |_| Msg::ClearLane(lane))}>{ "clear" }</button>
                                    </td>
                                </tr>
                            }
                        }) }
                    </table>
                    <p class="device-actions">
                        <button onclick={link.callback(|_| Msg::Bind(Some(DeviceId::Keyboard)))}>{ "bind arrows" }</button>
                        <button onclick={link.callback(|_| Msg::ResetKeys)}>{ "reset keys" }</button>
                    </p>
                </div>
                { for pads.iter().map(|(id, standard, count)| self.pad_block(link, id, *standard, *count)) }
                <label>
                    <input type="checkbox" checked={s.fast_pad_poll} onchange={fast} />
                    { " Poll controllers every millisecond during play (smoother timing, more CPU)" }
                </label>
                { if self.gamepads.is_none() { html! {
                    <p class="muted">{ "This browser does not support controllers." }</p>
                } } else { html! {} } }
            </section>
        }
    }

    /// One controller: its controls per lane, Start and Back, and whether
    /// they are saved, the standard defaults or missing. `count` is how
    /// many identical controllers are connected (0: none).
    fn pad_block(&self, link: &html::Scope<Self>, id: &str, standard: bool, count: usize) -> Html {
        let saved = self.settings.pads.iter().find(|p| p.id == id);
        let bindings = self.settings.pad_bindings(id, standard);
        let (status, class) = match (saved, &bindings) {
            (Some(p), _) if p.is_empty() => ("nothing bound", "missing"),
            (Some(_), _) => ("saved", ""),
            (None, Some(_)) => ("defaults (standard layout)", ""),
            (None, None) => ("not bound yet", "missing"),
        };
        let label = |c: &String| {
            c.parse::<Control>()
                .map(|c| c.label())
                .unwrap_or_else(|_| c.clone())
        };
        let list = |controls: &[String]| {
            if controls.is_empty() {
                "—".to_string()
            } else {
                controls.iter().map(label).collect::<Vec<_>>().join(", ")
            }
        };
        let b = bindings.unwrap_or_default();
        let rows = (0..4)
            .map(|lane| {
                (
                    LANE_NAMES[lane],
                    list(b.single.get(lane).map(Vec::as_slice).unwrap_or_default()),
                )
            })
            .chain([
                (
                    "Start",
                    b.start.as_ref().map(label).unwrap_or_else(|| "—".into()),
                ),
                (
                    "Back",
                    b.back.as_ref().map(label).unwrap_or_else(|| "—".into()),
                ),
            ]);
        let presence = match count {
            0 => "not connected".to_string(),
            1 => "connected".to_string(),
            n => format!("{n} connected"),
        };
        let (bind_id, clear_id, forget_id) = (id.to_string(), id.to_string(), id.to_string());
        html! {
            <div class={classes!("device-block", (count == 0).then_some("absent"))}>
                <div class="device-head">
                    <strong>{ id }</strong>
                    <span class={classes!("bind-status", class)}>{ status }</span>
                    <span class="muted">{ presence }</span>
                </div>
                <table class="keys-table">
                    { for rows.map(|(name, value)| html! {
                        <tr><td>{ name }</td><td><code>{ value }</code></td></tr>
                    }) }
                </table>
                <p class="device-actions">
                    <button onclick={link.callback(move |_| Msg::Bind(Some(DeviceId::Gamepad(bind_id.clone()))))}>{ "bind arrows" }</button>
                    <button disabled={b.is_empty()} onclick={link.callback(move |_| Msg::ClearPad(clear_id.clone()))}>{ "clear" }</button>
                    <button disabled={saved.is_none()} onclick={link.callback(move |_| Msg::ForgetPad(forget_id.clone()))}>{ "forget" }</button>
                </p>
            </div>
        }
    }

    fn volume_control(&self, link: &html::Scope<Self>) -> Html {
        let percent = (volume_to_slider(self.settings.volume) * 100.0).round() as i32;
        let db = if self.settings.volume > 0.0 {
            format!("{:.0} dB", 20.0 * self.settings.volume.log10())
        } else {
            "muted".into()
        };
        let oninput = link.callback(|e: InputEvent| {
            let v = e
                .target_dyn_into::<HtmlInputElement>()
                .and_then(|i| i.value().parse::<f32>().ok())
                .unwrap_or(0.0);
            Msg::Volume(slider_to_volume(v / 100.0))
        });
        let sample = match (&self.sample, &self.sample_song) {
            (Some(_), _) => html! {
                <button onclick={link.callback(|_| Msg::SampleEnded)}>{ "■ stop sample" }</button>
            },
            (None, Some(song)) => {
                let song = song.clone();
                let link = link.clone();
                // Started in the click handler itself (Safari).
                let onclick = Callback::from(move |_: MouseEvent| match Preview::start(&song) {
                    Ok(p) => link.send_message(Msg::SampleStarted(p)),
                    Err(e) => web_sys::console::warn_1(&format!("volume sample: {e}").into()),
                });
                html! { <button {onclick}>{ "▶ play a sample" }</button> }
            }
            (None, None) => html! {},
        };
        html! {
            <div class="volume-control">
                <label for="volume">{ "Music volume" }</label>
                <input id="volume" type="range" min="0" max="100" step="1" value={percent.to_string()} {oninput} />
                <span class="volume-value">{ format!("{percent}%") }</span>
                <span class="muted">{ db }</span>
                { sample }
            </div>
        }
    }

    fn reset_section(&self, link: &html::Scope<Self>) -> Html {
        let button = |r: Reset, label: &str, disabled: bool| {
            if self.confirm == Some(r) {
                html! { <button class="danger" onclick={link.callback(move |_| Msg::Reset(r))}>{ "click again to confirm" }</button> }
            } else {
                html! { <button {disabled} onclick={link.callback(move |_| Msg::AskReset(r))}>{ label.to_string() }</button> }
            }
        };
        let importing = import::status().0.is_some();
        let songs = match self.imported {
            0 => "no imported songs".to_string(),
            1 => "1 imported song".to_string(),
            n => format!("{n} imported songs"),
        };
        html! {
            <section>
                <h2>{ "Reset" }</h2>
                <table class="keys-table reset-table">
                    <tr>
                        <td>{ button(Reset::Settings, "reset settings", false) }</td>
                        <td class="muted">{ "Speed, scroll and note options, rules, volume, keys, controller bindings, display options and device calibrations back to defaults. Imported songs and per-song offsets are kept." }</td>
                    </tr>
                    <tr>
                        <td>{ button(Reset::Songs, "remove imported songs", self.imported == 0 || importing) }</td>
                        <td class="muted">{ format!("Deletes the songs imported into this browser ({songs}) and their offsets. Settings are kept.") }</td>
                    </tr>
                    <tr>
                        <td>{ button(Reset::Everything, "reset everything", importing) }</td>
                        <td class="muted">{ "Both of the above, plus the offsets of the bundled songs: like a first visit." }</td>
                    </tr>
                </table>
                { for self.notice.as_ref().map(|n| html! { <p>{ n }</p> }) }
            </section>
        }
    }
}

const SCROLL_ACTIONS: [(ScrollAction, &str); 4] = [
    (ScrollAction::Normal, "normal"),
    (ScrollAction::Boost, "boost: speeds up near the receptors"),
    (ScrollAction::Brake, "brake: slows down near the receptors"),
    (ScrollAction::Wave, "wave"),
];

const TURNS: [(Turn, &str); 5] = [
    (Turn::Off, "off"),
    (Turn::Mirror, "mirror"),
    (Turn::Left, "left"),
    (Turn::Right, "right"),
    (Turn::Shuffle, "shuffle: a new lane order each play"),
];

const APPEARANCES: [(Appearance, &str); 5] = [
    (Appearance::Visible, "normal"),
    (
        Appearance::Hidden,
        "hidden: notes vanish before the receptors",
    ),
    (Appearance::Sudden, "sudden: notes appear late"),
    (Appearance::HiddenSudden, "hidden + sudden"),
    (Appearance::Stealth, "stealth: no notes"),
];

const CUTS: [(TimingCut, &str); 3] = [
    (TimingCut::Off, "all"),
    (TimingCut::Quarters, "quarter notes only"),
    (TimingCut::Eighths, "quarters and eighths"),
];

const FIELD_FILTER_CHOICES: [(f32, &str); 4] = [
    (FIELD_FILTERS[0], "off"),
    (FIELD_FILTERS[1], "dark (40%)"),
    (FIELD_FILTERS[2], "darker (60%)"),
    (FIELD_FILTERS[3], "darkest (80%)"),
];

/// A select over fixed choices; option values are indices into `choices`.
fn choice<T: Copy + PartialEq + 'static>(
    current: T,
    choices: &'static [(T, &'static str)],
    on: Callback<T>,
) -> Html {
    let onchange = Callback::from(move |e: Event| {
        if let Some(&(v, _)) = e
            .target_dyn_into::<HtmlSelectElement>()
            .and_then(|s| s.value().parse::<usize>().ok())
            .and_then(|i| choices.get(i))
        {
            on.emit(v);
        }
    });
    html! {
        <select {onchange}>
            { for choices.iter().enumerate().map(|(i, (v, label))| html! {
                <option value={i.to_string()} selected={*v == current}>{ *label }</option>
            }) }
        </select>
    }
}

fn profile_table(
    kind: &'static str,
    profiles: &[crate::settings::OffsetProfile],
    link: &html::Scope<Options>,
) -> Html {
    if profiles.is_empty() {
        return html! { <p class="muted">{ format!("no {kind} profiles yet") }</p> };
    }
    html! {
        <table class="keys-table">
            { for profiles.iter().map(|p| {
                let id = p.id.clone();
                let id2 = p.id.clone();
                let oninput = link.callback(move |e: InputEvent| {
                    let v = e.target_dyn_into::<HtmlInputElement>().and_then(|i| i.value().parse::<f64>().ok()).unwrap_or(0.0);
                    Msg::ProfileOffsetMs(kind, id.clone(), v)
                });
                html! {
                    <tr>
                        <td>{ format!("{kind}: {}", p.label) }</td>
                        <td><input type="number" value={format!("{}", (p.offset * 1000.0).round())} step="1" min="-500" max="500" {oninput} />{ " ms" }</td>
                        <td class="muted">{ if p.calibrated { "calibrated" } else { "default" } }</td>
                        <td><button onclick={link.callback(move |_| Msg::DeleteProfile(kind, id2.clone()))}>{ "forget" }</button></td>
                    </tr>
                }
            }) }
        </table>
    }
}
