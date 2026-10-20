//! The game: the web shell (`ddi_web`) when built for the browser (Trunk,
//! through `index.html`), the desktop shell (`ddi_desktop`) everywhere
//! else (`cargo run`).

fn main() {
    #[cfg(target_arch = "wasm32")]
    ddi_web::start();
    #[cfg(not(target_arch = "wasm32"))]
    ddi_desktop::run();
}
