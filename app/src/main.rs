mod app;
mod boot;
mod calibration;
mod components;
mod crash;
mod game_loop;
mod import;
mod lyrics;
mod menu;
mod mods;
mod movies;
mod play;
mod preview;
mod router;
mod settings;
mod songs;
mod web;

fn main() {
    crash::install_hook();
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("document must exist");
    let root = document
        .get_element_by_id("app-root")
        .expect("#app-root must exist in index.html");
    yew::Renderer::<app::App>::with_root(root).render();
}
