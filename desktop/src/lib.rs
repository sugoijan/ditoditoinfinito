//! The desktop shell (`docs/plans/ui-core.md`, step 3): a winit window
//! running the UI core, drawn by `ddi_render`'s painter with cosmic-text
//! and the system's fonts. It runs the developer screen
//! (`ddi_ui::dev`), the same code as the web's `#/ui-dev`, and the
//! theme's screens on sample data (song select by default, `--screen
//! select|results|options` as `#/ui-dev?screen=…`; `--dev` for the
//! developer screen); audio, play and the library on disk come
//! with the rest of phase 9.
//!
//! - [`keys`]: winit's keys as the DOM names them.
//! - [`store`]: settings in a file, through `ddi_platform::storage`.
//! - [`gpu`]: the painter over a surface or an offscreen texture, with
//!   the bundled faces (the HUD's, the theme's) and the system's.
//! - [`images`]: banners and backgrounds for the texture cache.
//! - [`os`]: links, file dialogs, the clipboard.
//! - [`shell`]: the window and its events.

pub mod gpu;
pub mod images;
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

/// Runs the desktop app: `ditoditoinfinito [--screen select|results|options |
/// --dev | --widgets] [--seconds N]`, the theme's screens on sample data in a
/// window, song select unless `--screen` names another (`--dev`: the UI
/// core's developer screen; `--widgets`: its widget screen); with
/// `--seconds`, it quits after that long and prints what it drew.
pub fn run() {
    store::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let screen = match args.iter().position(|a| a == "--screen") {
        Some(i) => args.get(i + 1).cloned().unwrap_or_default(),
        None if args.iter().any(|a| a == "--widgets") => "widgets".into(),
        None if args.iter().any(|a| a == "--dev") => "dev".into(),
        None => "select".into(),
    };
    let mut shell = shell::Shell::named(&screen);
    if let Some(i) = args.iter().position(|a| a == "--seconds") {
        shell.quit_after = args.get(i + 1).and_then(|s| s.parse().ok());
    }
    let event_loop = match winit::event_loop::EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("no event loop: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = event_loop.run_app(&mut shell) {
        eprintln!("event loop: {e}");
    }
    if let Some(e) = &shell.error {
        eprintln!("{e}");
        std::process::exit(1);
    }
    println!(
        "frames {} · redraws asked {} · skipped while occluded {} · status {:?}",
        shell.frames,
        shell.asked,
        shell.occluded_skips,
        shell.dev.borrow().status,
    );
    if let Some((env, _)) = &shell.theme {
        let s = env.state.borrow();
        println!(
            "theme: screen {} · song {:?} · style {} · chart {:?} · compact {} · pulse {}",
            s.screen, s.song, s.style, s.chart, s.compact, s.pulsing
        );
    }
}
