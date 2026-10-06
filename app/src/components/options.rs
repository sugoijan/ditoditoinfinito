//! Options screen: speed, scroll, ruleset, offsets, volume, key bindings.
//! Saved to `localStorage` on every change.

use gloo::events::EventListener;
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, HtmlSelectElement, KeyboardEvent};
use yew::prelude::*;

use crate::router::Route;
use crate::settings::Settings;

pub(crate) enum Msg {
    Speed(f64),
    Reverse(bool),
    Ruleset(String),
    AudioOffsetMs(f64),
    VisualOffsetMs(f64),
    Volume(f32),
    Debug(bool),
    ShowDeltas(bool),
    ReceptorSnap(bool),
    /// Edit a profile's offset (kind "audio"/"display", id, ms).
    ProfileOffsetMs(&'static str, String, f64),
    DeleteProfile(&'static str, String),
    /// Start listening for a key to bind to `lane`.
    Capture(usize),
    Captured(String),
    ClearLane(usize),
    ResetKeys,
    Reset,
}

pub(crate) struct Options {
    settings: Settings,
    capturing: Option<usize>,
    _keys: Option<EventListener>,
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
        Options {
            settings: Settings::load(),
            capturing: None,
            _keys: keys,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Speed(v) => self.settings.speed = v.clamp(0.25, 10.0),
            Msg::Reverse(v) => self.settings.reverse = v,
            Msg::Ruleset(id) => self.settings.ruleset = id,
            Msg::AudioOffsetMs(ms) => self.settings.audio_offset = (ms / 1000.0).clamp(-0.5, 0.5),
            Msg::VisualOffsetMs(ms) => self.settings.visual_offset = (ms / 1000.0).clamp(-0.5, 0.5),
            Msg::Volume(v) => self.settings.volume = v.clamp(0.0, 1.0),
            Msg::Debug(v) => self.settings.debug = v,
            Msg::ShowDeltas(v) => self.settings.show_deltas = v,
            Msg::ReceptorSnap(v) => self.settings.receptor_snap = v,
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
            Msg::Reset => self.settings = Settings::default(),
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
                </section>
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
                    <label>{ "Volume " }{ number((f64::from(s.volume) * 100.0).round() / 100.0, "0.05", "0", "1", link.callback(|v: f64| Msg::Volume(v as f32))) }</label>
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
                    <p class="muted">
                        { "Diagnostics: " }
                        <a href={Route::Play { song: "some-things-must".into(), chart: 2, force_gl: false, auto: true, bias_ms: 0 }.to_hash()}>{ "autoplay demo" }</a>
                        { " · " }
                        <a href={Route::Play { song: "some-things-must".into(), chart: 2, force_gl: true, auto: true, bias_ms: 0 }.to_hash()}>{ "autoplay demo on WebGL2" }</a>
                        { " (forces the fallback renderer)" }
                    </p>
                </section>
                <section>
                    <h2>{ "Keys (4 panels)" }</h2>
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
                    <p><button onclick={link.callback(|_| Msg::ResetKeys)}>{ "reset keys" }</button></p>
                </section>
                <p>
                    <button onclick={link.callback(|_| Msg::Reset)}>{ "reset everything" }</button>
                    { " · " }
                    <a href={Route::Home.to_hash()}>{ "← back" }</a>
                </p>
            </main>
        }
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
