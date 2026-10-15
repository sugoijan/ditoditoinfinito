//! `ditoditoinfinito-desktop [--widgets] [--seconds N]`: the UI core's
//! developer screen in a window (`--widgets`: the widget screen); with
//! `--seconds`, it quits after that long and prints what it drew.

use ddi_desktop::shell::Shell;

fn main() {
    ddi_desktop::store::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut shell = Shell::with_screen(args.iter().any(|a| a == "--widgets"));
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
}
