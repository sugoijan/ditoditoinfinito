//! The web game's own error reports (the stats proposal's step 4,
//! `errors-v1`, `ddi_room::error_report`): a panic (from the panic hook,
//! before the loader's crash page), a promise rejected with nobody to
//! catch it, a lost graphics device and the no-GPU page, each as a kind,
//! the route's kind, this client's kind and its message scrubbed, sent with
//! `navigator.sendBeacon` to `<api>/stats/error` (never waited for, never
//! sent again, nothing if it fails). The loader (`site/boot.js`) reports
//! its own: a wasm trap and the page unable to start the game.
//!
//! None is sent where Options → Data and privacy → Send error reports is Off or the
//! browser sends the Global Privacy Control signal; at most one a
//! fingerprint and five a session (`ddi_room::error_report::Sent` in
//! `sessionStorage`, shared with the loader). The desktop app sends none.
//!
//! Read with care: a report may be made inside the panic hook, so nothing
//! here borrows the game's state (the settings are read from storage as
//! stored, the screen's graphics only where nothing holds it).

use ddi_room::error_report::{self as er, SENT_KEY, Sent};
use js_sys::Reflect;
use wasm_bindgen::JsValue;

/// The browser sends the Global Privacy Control signal
/// (`navigator.globalPrivacyControl`).
pub(crate) fn privacy_signal() -> bool {
    web_sys::window()
        .and_then(|w| Reflect::get(&w.navigator(), &"globalPrivacyControl".into()).ok())
        .is_some_and(|v| v.as_bool() == Some(true))
}

/// Options → Data and privacy → Send error reports as stored (On where nothing says
/// Off, as its default).
fn row_on() -> bool {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return true;
    };
    let stored = storage
        .get_item(ddi_ui::state::settings::KEY)
        .ok()
        .flatten();
    stored
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("error_reports").and_then(serde_json::Value::as_bool))
        .unwrap_or(true)
}

/// The route's kind, from the address (never the address).
fn route() -> &'static str {
    let hash = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default();
    er::route_kind(&hash)
}

/// Reports `kind` with its message `raw` (scrubbed here, and again by the
/// server), where the player lets it and this session has not sent it.
pub(crate) fn report(kind: &str, raw: &str) {
    if !er::may_send(row_on(), privacy_signal()) {
        return;
    }
    let Some(report) = er::report(kind, route(), Some(crate::online::client_kind()), raw)
    else {
        return;
    };
    let Some(window) = web_sys::window() else {
        return;
    };
    let session = window.session_storage().ok().flatten();
    let mut sent = Sent::read(
        session
            .as_ref()
            .and_then(|s| s.get_item(SENT_KEY).ok().flatten())
            .as_deref(),
    );
    if !sent.allow(&report.fingerprint()) {
        return;
    }
    if let Some(s) = &session {
        let _ = s.set_item(SENT_KEY, &sent.text());
    }
    let url = format!("{}/stats/error", crate::share::base());
    let _ = window
        .navigator()
        .send_beacon_with_opt_str(&url, Some(&report.text()));
    count_sent(kind);
}

/// Reports sent this page, by kind (`window.__DDI_ERRORS`, for the verify
/// skill's checks; nothing of their text).
fn count_sent(kind: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let key = JsValue::from_str("__DDI_ERRORS");
    let obj = Reflect::get(&window, &key)
        .ok()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| {
            let o: JsValue = js_sys::Object::new().into();
            let _ = Reflect::set(&window, &key, &o);
            o
        });
    let k = JsValue::from_str(kind);
    let n = Reflect::get(&obj, &k).ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    let _ = Reflect::set(&obj, &k, &JsValue::from_f64(n + 1.0));
}

/// A promise rejected with nobody to catch it, reported once the game
/// runs (the loader takes a wasm trap's itself, as `wasm_trap`).
pub(crate) fn watch_rejections() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    let Some(window) = web_sys::window() else {
        return;
    };
    let on = Closure::<dyn Fn(web_sys::Event)>::new(|event: web_sys::Event| {
        let reason = Reflect::get(&event, &"reason".into()).unwrap_or(JsValue::UNDEFINED);
        let trap = reason.is_instance_of::<js_sys::WebAssembly::RuntimeError>();
        if trap {
            return;
        }
        let text = Reflect::get(&reason, &"message".into())
            .ok()
            .and_then(|m| m.as_string())
            .or_else(|| reason.as_string())
            .or_else(|| {
                js_sys::JSON::stringify(&reason)
                    .ok()
                    .and_then(|s| s.as_string())
            })
            .unwrap_or_default();
        let name = Reflect::get(&reason, &"name".into())
            .ok()
            .and_then(|m| m.as_string())
            .unwrap_or_default();
        let raw = if name.is_empty() {
            text
        } else {
            format!("{name}: {text}")
        };
        report("unhandled_rejection", &raw);
    });
    let _ = window.add_event_listener_with_callback("unhandledrejection", on.as_ref().unchecked_ref());
    on.forget();
}

/// `__DDI_TEST_ERROR(kind, message)`: a report as the game would make it,
/// for the verify skill (the scrub, the switch, Global Privacy Control and
/// the server's counting checked on a real page). Only where automation
/// drives the page (`navigator.webdriver`), as `__DDI_DEBUG`'s eager
/// publishing; it sends nothing the player has not let it send.
pub(crate) fn install_test_hook() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    let Some(window) = web_sys::window() else {
        return;
    };
    if !crate::web::automated() {
        return;
    }
    let f = Closure::<dyn Fn(JsValue, JsValue)>::new(|kind: JsValue, message: JsValue| {
        report(
            &kind.as_string().unwrap_or_default(),
            &message.as_string().unwrap_or_default(),
        );
    });
    let _ = Reflect::set(&window, &"__DDI_TEST_ERROR".into(), f.as_ref().unchecked_ref());
    f.forget();
}
