//! The desktop shell (`docs/plans/ui-core.md`, step 3): a winit window
//! running the UI core, drawn by `ddi_render`'s painter with cosmic-text
//! and the system's fonts. It runs the developer screen
//! (`ddi_ui::dev`), the same code as the web's `#/ui-dev`; audio, play
//! and the library on disk come with the rest of phase 9.
//!
//! - [`keys`]: winit's keys as the DOM names them.
//! - [`store`]: settings in a file, through `ddi_platform::storage`.
//! - [`gpu`]: the painter over a surface or an offscreen texture.
//! - [`os`]: links, file dialogs, the clipboard.
//! - [`shell`]: the window and its events.

pub mod gpu;
pub mod keys;
pub mod os;
pub mod shell;
pub mod store;

/// How desktop folds text for the search: Unicode's tables (the web asks
/// the browser).
pub use ddi_library::search::Nfkd;

/// Polls a future to its end on this thread (wgpu's native requests are
/// ready at once or soon; no executor needed).
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}
