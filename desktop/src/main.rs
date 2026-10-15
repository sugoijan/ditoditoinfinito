//! `ditoditoinfinito-desktop [--screen select|results|options | --dev |
//! --widgets] [--seconds N]`: the theme's screens on sample data in a
//! window, song select unless `--screen` names another (`--dev`: the UI
//! core's developer screen; `--widgets`: its widget screen); with
//! `--seconds`, it quits after that long and prints what it drew.

use ddi_desktop::shell::Shell;

fn main() {
    ddi_desktop::store::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let screen = match args.iter().position(|a| a == "--screen") {
        Some(i) => args.get(i + 1).cloned().unwrap_or_default(),
        None if args.iter().any(|a| a == "--widgets") => "widgets".into(),
        None if args.iter().any(|a| a == "--dev") => "dev".into(),
        None => "select".into(),
    };
    let mut shell = Shell::named(&screen);
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
