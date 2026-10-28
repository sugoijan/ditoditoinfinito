//! The web shell: our theme's screens on one wgpu canvas and device for
//! the whole app (pages as components of our own, `component`), Web
//! Audio, settings in `localStorage`, songs in IndexedDB. Compiles for `wasm32-unknown-unknown` only; the
//! `ddi` crate (`app/`) starts it there, and the desktop
//! shell everywhere else. Built natively (an editor's analysis, a
//! workspace-wide `cargo check`) it is empty.
#![cfg(target_arch = "wasm32")]

mod app;
mod boot;
mod calibration;
mod component;
mod components;
mod crash;
mod editor;
mod editor_audio;
mod editor_field;
mod field_preview;
mod game_loop;
mod history;
mod import;
mod mods;
mod movies;
mod noteskins;
mod notices;
mod play;
mod preload;
mod preview;
mod preview_movie;
mod replay;
mod router;
mod scripts;
mod select_data;
mod select_preview;
mod settings;
mod songs;
mod stage;
mod stills;
mod web;

/// Starts the app in the page (`#app-root`).
pub fn start() {
    crash::install_hook();
    router::reset_start_focus();
    // Settings and the song list's place, from the browser's storage.
    web::kv::install();
    // The app's canvas and device, made once for every route.
    web::screen::boot();
    // The theme's faces, which the first screen waits for (`app::start`).
    web::text::load_theme_fonts();
    // The play history's summaries, and stale ones worked out again.
    history::start();
    // The chosen noteskin fetched in the background, for the first play.
    noteskins::warm();
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("document must exist");
    let root = document
        .get_element_by_id("app-root")
        .expect("#app-root must exist in index.html");
    app::start(root);
}
