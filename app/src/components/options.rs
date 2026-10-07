//! Options screen: speed, scroll, ruleset, offsets, volume (with a sample
//! to judge it by), key bindings, and resetting settings or the imported
//! songs. Settings are saved to `localStorage` on every change.

use gloo::events::EventListener;
use wasm_bindgen::JsCast;
use web_sys::{AudioBuffer, HtmlInputElement, HtmlSelectElement, KeyboardEvent};
use yew::prelude::*;

use crate::import;
use crate::preview::Preview;
use crate::router::Route;
use crate::settings::{Settings, slider_to_volume, volume_to_slider};
use crate::songs::{Library, ManifestEntry, clear_imported, is_imported_id};

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
        ctx.link().send_future(async {
            match Library::load().await {
                Ok(l) => Msg::Library(l.bundled.into_iter().next(), l.imported.len()),
                Err(_) => Msg::Library(None, 0),
            }
        });
        Options {
            settings: Settings::load(),
            capturing: None,
            sample_song: None,
            sample: None,
            imported: 0,
            confirm: None,
            notice: None,
            _keys: keys,
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
                { self.reset_section(link) }
                <p><a href={Route::Home.to_hash()}>{ "← back" }</a></p>
            </main>
        }
    }
}

impl Options {
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
                        <td class="muted">{ "Speed, rules, volume, keys, display options and device calibrations back to defaults. Imported songs and per-song offsets are kept." }</td>
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
