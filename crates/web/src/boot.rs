//! Bridge to the boot shell defined inline in `index.html`. All calls are
//! best-effort: if the shell is missing (tests) they silently no-op.

use std::cell::Cell;

use js_sys::{Function, Reflect};
use wasm_bindgen::{JsCast, JsValue};

thread_local! {
    static READY_SENT: Cell<bool> = const { Cell::new(false) };
    /// The shell's fade began, the bar's fill done: the screens held
    /// under the shell come in now ([`revealed`], `Ui::reveal`).
    static REVEALED: Cell<bool> = const { Cell::new(false) };
    /// A UI host is coming (the play route's start prompt, once its
    /// song is loaded): [`ready_when_settled`] waits for it rather than
    /// fading with none up.
    static EXPECTED: Cell<bool> = const { Cell::new(false) };
    /// [`ready_when_settled`] was asked: the UI host drawing fades the
    /// shell once its screen is at rest.
    static WAITING: Cell<bool> = const { Cell::new(false) };
    /// The bar's start progress when [`ready_when_settled`] was asked:
    /// the stretch left is the first screen's ([`settling`]).
    static BASE: Cell<f64> = const { Cell::new(1.0) };
    /// Units of the preload beside the textures (`crate::preload`: the
    /// manifest, the skins' index, the skin), each weighted by the
    /// milliseconds it is expected to take, so the bar moves at one
    /// pace through them: asked for, and done.
    static UNITS: Cell<(f64, f64)> = const { Cell::new((0.0, 0.0)) };
    /// Part of a unit's weight in already (its files so far), for the
    /// bar: cleared when the unit is done ([`unit_partial`]).
    static PARTIAL: Cell<f64> = const { Cell::new(0.0) };
}

/// What a texture of the first screen is expected to take, ms (a fetch
/// and a decode, several at once), and the screen's rest after.
const TEXTURE_MS: f64 = 120.0;
const SETTLE_MS: f64 = 150.0;

/// `ms` of a unit still loading is in: how far it is, for the bar.
pub(crate) fn unit_partial(ms: f64) {
    PARTIAL.with(|p| p.set(ms.max(0.0)));
}

/// A unit expected to take `ms` the shell waits for.
pub(crate) fn add_unit(ms: f64) {
    UNITS.with(|u| {
        let (total, done) = u.get();
        u.set((total + ms.max(0.0), done));
    });
}

/// A unit added as `from` ms is now expected to take `to`.
pub(crate) fn resize_unit(from: f64, to: f64) {
    UNITS.with(|u| {
        let (total, done) = u.get();
        u.set(((total - from + to).max(done), done));
    });
}

/// A unit of `ms` in.
pub(crate) fn unit_done(ms: f64) {
    UNITS.with(|u| {
        let (total, done) = u.get();
        u.set((total, (done + ms).min(total)));
    });
    PARTIAL.with(|p| p.set(0.0));
}

/// The link's bandwidth as the shell measured it downloading the game,
/// bytes a second (`__DDI_BOOT.bandwidth`); `None` until it did.
pub(crate) fn bandwidth() -> Option<f64> {
    let mut found = None;
    with_boot(|boot| {
        found = Reflect::get(boot, &JsValue::from_str("bandwidth"))
            .ok()
            .and_then(|v| v.as_f64())
            .filter(|b| *b > 0.0);
    });
    found
}

fn with_boot<F: FnOnce(&js_sys::Object)>(action: F) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(value) = Reflect::get(&window, &JsValue::from_str("__DDI_BOOT")) else {
        return;
    };
    if value.is_null() || value.is_undefined() {
        return;
    }
    let Ok(obj) = value.dyn_into::<js_sys::Object>() else {
        return;
    };
    action(&obj);
}

fn call(method: &str, args: &[JsValue]) -> Option<JsValue> {
    let mut result = None;
    with_boot(|boot| {
        let Ok(value) = Reflect::get(boot, &JsValue::from_str(method)) else {
            return;
        };
        let Ok(func) = value.dyn_into::<Function>() else {
            return;
        };
        let array = js_sys::Array::new();
        for arg in args {
            array.push(arg);
        }
        result = func.apply(boot, &array).ok();
    });
    result
}

/// Reports a Rust panic to the boot shell, which owns recovery (overlay +
/// reload). The wasm instance is unusable once this is called.
pub(crate) fn crash(message: &str) {
    call("crash", &[JsValue::from_str(message)]);
}

/// How far the game readied what its first screen needs, 0 to 1 (the
/// bar's part after the download).
pub(crate) fn progress(ratio: f64) {
    call("setStartProgress", &[JsValue::from_f64(ratio)]);
}

/// Fades out the boot shell, once its bar has filled (the shell says
/// how long that takes); the screens held under it come in as the fade
/// begins. Idempotent.
pub(crate) fn ready() {
    let already_sent = READY_SENT.with(|flag| flag.replace(true));
    if already_sent {
        return;
    }
    WAITING.with(|w| w.set(false));
    crate::preload::finished();
    let fill = call("ready", &[])
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, 2000.0);
    gloo::timers::callback::Timeout::new(fill as u32, || {
        REVEALED.with(|r| r.set(true));
        crate::components::ui_dev::wake_all();
    })
    .forget();
}

/// Whether the shell's fade has begun: a host's screens held under it
/// come in now (`Ui::reveal`).
pub(crate) fn revealed() -> bool {
    REVEALED.with(Cell::get)
}

/// A UI host is coming (the play route's prompt, after its song loads):
/// the shell waits for it as for a page's.
pub(crate) fn expect_host() {
    EXPECTED.with(|e| e.set(true));
}

/// Whether [`expect_host`] was called: the first screen is the play
/// route's prompt.
pub(crate) fn expected() -> bool {
    EXPECTED.with(Cell::get)
}

/// Fades out the boot shell once the screen under it is at rest with
/// its textures in: the shell's title and rain stand where the start
/// menu's land, on the same clock, so the fade shows the menu whole in
/// place rather than its title sliding in under a title, or its rain
/// arriving after the loader's went. `base` is the bar's start progress
/// now: the stretch left is the screen's, moved by [`settling`]. Where
/// no UI host draws (a plain page, a play), now; and never later than a
/// few seconds, should a texture never come (the shell's art runs on
/// meanwhile).
pub(crate) fn ready_when_settled(base: f64) {
    if READY_SENT.with(Cell::get) {
        return;
    }
    if crate::components::ui_dev::hosts() == 0 && !EXPECTED.with(Cell::get) {
        ready();
        return;
    }
    BASE.with(|b| b.set(base.clamp(0.0, 1.0)));
    WAITING.with(|w| w.set(true));
    // Should something never come; the prompt waits longer for its skin
    // (`crate::preload`).
    let cap = if expected() { 12_000 } else { 8000 };
    gloo::timers::callback::Timeout::new(cap, ready).forget();
}

/// Each frame while the shell waits for the first screen (`settled`:
/// at rest): the bar's last stretch, over the textures asked for, the
/// preload's units and the rest; whether all is in, so the shell can
/// fade.
pub(crate) fn settling(settled: bool) -> bool {
    if !waiting() {
        return false;
    }
    let (requested, decoded) = crate::web::textures::counts();
    let (units, units_done) = UNITS.with(Cell::get);
    let total = requested as f64 * TEXTURE_MS + units + SETTLE_MS;
    let partial = PARTIAL.with(Cell::get).min(units - units_done);
    let done = decoded.min(requested) as f64 * TEXTURE_MS
        + units_done
        + partial
        + if settled { SETTLE_MS } else { 0.0 };
    let base = BASE.with(Cell::get);
    progress(base + (1.0 - base) * (done / total).clamp(0.0, 1.0));
    settled && decoded >= requested && units_done >= units
}

/// Whether [`ready_when_settled`] waits for a settled screen
/// (`ui_dev`'s frame calls [`ready`] then).
pub(crate) fn waiting() -> bool {
    WAITING.with(Cell::get)
}

/// Whether the shell is still up, its title showing: a start menu built
/// now comes up under it (`StartScreen::in_place`).
pub(crate) fn shown() -> bool {
    !READY_SENT.with(Cell::get)
}
