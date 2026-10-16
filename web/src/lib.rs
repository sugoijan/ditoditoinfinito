//! The web shell: Yew for the menus still on the DOM, one wgpu canvas
//! and device for the whole app, Web Audio, settings in `localStorage`,
//! songs in IndexedDB. Compiles for `wasm32-unknown-unknown` only; the
//! `ditoditoinfinito` crate (`app/`) starts it there, and the desktop
//! shell everywhere else. Built natively (an editor's analysis, a
//! workspace-wide `cargo check`) it is empty.
#![cfg(target_arch = "wasm32")]

mod app;
mod boot;
mod calibration;
mod components;
mod crash;
mod game_loop;
mod history;
mod import;
mod lyrics;
mod mods;
mod movies;
mod noteskins;
mod play;
mod preview;
mod router;
mod select_data;
mod select_preview;
mod settings;
mod songs;
mod stage;
mod web;

/// Starts the app in the page (`#app-root`).
pub fn start() {
    crash::install_hook();
    // Settings and the song list's place, from the browser's storage.
    web::kv::install();
    // The app's canvas and device, made once for every route.
    web::screen::boot();
    // The play history's summaries, and stale ones worked out again.
    history::start();
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("document must exist");
    let root = document
        .get_element_by_id("app-root")
        .expect("#app-root must exist in index.html");
    yew::Renderer::<app::App>::with_root(root).render();
}
