//! The web's chart rating worker (`docs/plans/msd-ratings.md`): an
//! imported song's simfile parsed as the library parses it (its generated
//! versions added, `ddi_chart`) and the four-lane charts asked for rated
//! (`ddi_msd`), off the page's thread. The page starts one only while a
//! chart ratings option wants ratings, sends it a song at a time and never
//! during a play (`crates/web/src/ratings.rs`).
//!
//! Protocol (the page's boot script loads this module from the compiled
//! `WebAssembly.Module` the page sends): the worker says `{ready: true}`
//! once it listens; the page sends `{id, text, ext, charts}` (the simfile's
//! text and extension, the charts' places in the parsed song); the worker
//! answers `{id, rated, ms}`, `rated` an array of `[index, digest, values]`
//! (the digest 32 hex digits, `values` the eight numbers or `null`), or
//! `{id, error}`. `ms` is the time it took.

#[cfg(target_arch = "wasm32")]
fn main() {
    use js_sys::{Array, Object, Reflect};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let set = |o: &Object, k: &str, v: &JsValue| {
        let _ = Reflect::set(o, &JsValue::from_str(k), v);
    };
    let reply = scope.clone();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let data = event.data();
        let get = |k: &str| Reflect::get(&data, &JsValue::from_str(k)).ok();
        let id = get("id").unwrap_or(JsValue::UNDEFINED);
        let text = get("text").and_then(|t| t.as_string()).unwrap_or_default();
        let ext = get("ext").and_then(|t| t.as_string()).unwrap_or_default();
        let charts: Vec<usize> = get("charts")
            .map(|c| Array::from(&c))
            .map(|a| a.iter().filter_map(|v| v.as_f64()).map(|v| v as usize).collect())
            .unwrap_or_default();
        let began = js_sys::Date::now();
        let msg = Object::new();
        set(&msg, "id", &id);
        match ddi_chart::formats::sm::parse_simfile(&text, &ext.to_ascii_lowercase()) {
            Ok(mut song) => {
                song.add_chart_versions();
                let rated = Array::new();
                for index in charts {
                    let Some(chart) = song.charts.get(index) else {
                        continue;
                    };
                    let entry = Array::new();
                    entry.push(&JsValue::from(index as f64));
                    entry.push(&JsValue::from_str(&song.chart_digest(index).to_string()));
                    match ddi_msd::chart::chart_msd(&song, chart) {
                        Some(v) => {
                            let values = Array::new();
                            for x in v {
                                values.push(&JsValue::from(f64::from(x)));
                            }
                            entry.push(&values);
                        }
                        None => {
                            entry.push(&JsValue::NULL);
                        }
                    }
                    rated.push(&entry);
                }
                set(&msg, "rated", &rated);
            }
            Err(e) => set(&msg, "error", &JsValue::from_str(&e.to_string())),
        }
        set(&msg, "ms", &JsValue::from(js_sys::Date::now() - began));
        let _ = reply.post_message(&msg);
    });
    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    let ready = Object::new();
    set(&ready, "ready", &JsValue::TRUE);
    let _ = scope.post_message(&ready);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
