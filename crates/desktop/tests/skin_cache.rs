//! Two reads of one skin draw its SVGs once: the start prompt's diagram's
//! read and the play's, with their own layouts and Lua environments, in
//! turn (the second finds the first's drawings) or at once (the second
//! waits for the first; Infinito's images take long enough for the two
//! to overlap otherwise) (`docs/plans/shell-sharing.md`, item 1). One
//! test: the count of SVGs drawn is the process's.

mod common;

use ddi_chart::Layout;
use ddi_desktop::play::skins::{Prepared, build, forget_drawings, prepare, rasterised, read};
use ddi_noteskin::lua::Env;
use ddi_ui::screens::prompt_controls::Controls;
use ddi_ui::state::settings::Settings;

/// Its images' pixels, in order of their hashes.
fn pixels(p: &Prepared) -> Vec<u64> {
    let mut v: Vec<u64> = p
        .images
        .iter()
        .flatten()
        .map(|i| ddi_noteskin::report::pixels_hash(i.size, &i.data))
        .collect();
    v.sort();
    v
}

#[test]
fn a_skins_second_read_draws_nothing_again() {
    common::indexes();
    let skins = ddi_desktop::library::noteskins::bundled();
    if skins.is_empty() {
        eprintln!("no bundled noteskins' index: skipped");
        return;
    }
    let play = Layout::dance_single();
    let controls = Controls::new(&play, &Settings::default());
    let (_, want, diagram) = ddi_shell::receptor_skin(&controls);
    // The demo's environment for the diagram, the default for the play.
    let diagram_env = want.env(&diagram);

    // In turn: the diagram's, then the play's.
    let key = ddi_noteskin::bundled::INFINITO;
    let before = rasterised();
    let first = prepare(key, &diagram, diagram_env.clone(), &skins).expect("Infinito reads");
    let once = rasterised() - before;
    assert!(once > 0);
    let second = prepare(key, &play, Env::default(), &skins).expect("Infinito reads");
    assert_eq!(rasterised() - before, once, "nothing drawn again");
    assert_eq!(pixels(&first), pixels(&second));

    // At once, nothing kept: the second waits for the first, then draws
    // nothing more than one read alone does.
    forget_drawings();
    let before = rasterised();
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(|| read(key, &diagram, diagram_env.clone(), &skins));
        let b = s.spawn(|| read(key, &play, Env::default(), &skins));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(rasterised() - before, once);
    let a = build(a.expect("Infinito reads"), &diagram).expect("Infinito builds");
    let b = build(b.expect("Infinito reads"), &play).expect("Infinito builds");
    assert_eq!(pixels(&a), pixels(&first));
    assert_eq!(pixels(&b), pixels(&first));
}
