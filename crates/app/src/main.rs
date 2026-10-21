//! The game: the web shell (`ddi_web`) when built for the browser (Trunk,
//! through `index.html`), the desktop shell (`ddi_desktop`) everywhere
//! else (`cargo run`).

// A Windows build outside debug opens no console window beside the game;
// output redirected to a file or a pipe still reaches it, and a run given
// flags prints in the console it was started from (`ddi_desktop::run`).
// `build.rs` gives the Windows executable its icon and version.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_arch = "wasm32")]
    ddi_web::start();
    #[cfg(not(target_arch = "wasm32"))]
    ddi_desktop::run();
}
