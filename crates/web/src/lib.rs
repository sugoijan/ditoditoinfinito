//! The web shell: our theme's screens on one wgpu canvas and device for
//! the whole app (pages as components of our own, `component`), Web
//! Audio, settings in `localStorage`, songs in IndexedDB. Compiles for `wasm32-unknown-unknown` only; the
//! `ddi` crate (`app/`) starts it there, and the desktop
//! shell everywhere else. Built natively (an editor's analysis, a
//! workspace-wide `cargo check`) it is empty but for the routes' table
//! (`route`: pure, so that its tests run natively).

#[cfg(target_arch = "wasm32")]
mod announcers;
#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod boot;
#[cfg(target_arch = "wasm32")]
mod calibration;
#[cfg(target_arch = "wasm32")]
mod chat;
#[cfg(target_arch = "wasm32")]
mod component;
#[cfg(target_arch = "wasm32")]
mod components;
#[cfg(target_arch = "wasm32")]
mod cpu;
#[cfg(target_arch = "wasm32")]
mod crash;
#[cfg(target_arch = "wasm32")]
mod errors;
#[cfg(target_arch = "wasm32")]
mod editor;
#[cfg(target_arch = "wasm32")]
mod editor_audio;
#[cfg(target_arch = "wasm32")]
mod editor_field;
#[cfg(target_arch = "wasm32")]
mod fake_room;
#[cfg(target_arch = "wasm32")]
mod fake_seat;
#[cfg(target_arch = "wasm32")]
mod field_preview;
#[cfg(target_arch = "wasm32")]
mod game_loop;
#[cfg(target_arch = "wasm32")]
mod generated;
#[cfg(target_arch = "wasm32")]
mod history;
#[cfg(target_arch = "wasm32")]
mod import;
#[cfg(target_arch = "wasm32")]
mod mods;
#[cfg(target_arch = "wasm32")]
mod movies;
#[cfg(target_arch = "wasm32")]
mod noteskin_cache;
#[cfg(target_arch = "wasm32")]
mod noteskins;
#[cfg(target_arch = "wasm32")]
mod online;
#[cfg(target_arch = "wasm32")]
mod perf;
#[cfg(target_arch = "wasm32")]
mod play;
#[cfg(target_arch = "wasm32")]
mod preload;
#[cfg(target_arch = "wasm32")]
mod preview;
#[cfg(target_arch = "wasm32")]
mod preview_movie;
#[cfg(target_arch = "wasm32")]
mod ratings;
#[cfg(target_arch = "wasm32")]
mod replay;
#[cfg(target_arch = "wasm32")]
mod room;
/// An online room's songs downloaded ahead (stage 2 of its song list).
#[cfg(target_arch = "wasm32")]
mod room_store;
// The routes' table is pure and natively tested (`route`); the page's
// address and history around it are the browser's (`router`).
// (Natively only its tests use it.)
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod route;
#[cfg(target_arch = "wasm32")]
mod router;
#[cfg(target_arch = "wasm32")]
mod scripts;
#[cfg(target_arch = "wasm32")]
mod select_data;
#[cfg(target_arch = "wasm32")]
mod select_preview;
#[cfg(target_arch = "wasm32")]
mod settings;
#[cfg(target_arch = "wasm32")]
mod share;
#[cfg(target_arch = "wasm32")]
mod songs;
#[cfg(target_arch = "wasm32")]
mod stage;
#[cfg(target_arch = "wasm32")]
mod stills;
#[cfg(target_arch = "wasm32")]
mod studio_scene;
#[cfg(target_arch = "wasm32")]
mod ui_host;
#[cfg(target_arch = "wasm32")]
mod web;

/// Starts the app in the page (`#app-root`).
#[cfg(target_arch = "wasm32")]
pub fn start() {
    crash::install_hook();
    router::reset_start_focus();
    // Settings and the song list's place, from the browser's storage.
    web::kv::install();
    // Round TW6: the live chat's side (Twitch's EventSub socket); a play
    // shows it only in a multiplayer stage where the room and the settings
    // say (round TW7).
    chat::install_live(None, None);
    // The game's own error reports: a promise nobody caught (a panic's
    // come from the hook above).
    errors::watch_rejections();
    errors::install_test_hook();
    // Announcers' scripts: the page's clock behind their fuel, and a seed
    // they cannot know for their tables' hashing.
    ddi_library::announcer::script::set_host(
        || js_sys::Date::now() / 1000.0,
        (js_sys::Math::random() * 9_007_199_254_740_991.0) as u64,
    );
    // The UI's language: the settings' choice, else the browser's.
    let languages: Vec<String> = web_sys::window()
        .map(|w| {
            w.navigator()
                .languages()
                .iter()
                .filter_map(|v| v.as_string())
                .collect()
        })
        .unwrap_or_default();
    ddi_l10n::set_system_languages(languages);
    settings::Settings::load().choose_language();
    ratings::kick();
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
