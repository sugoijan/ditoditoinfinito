//! The theme's UI as the window makes it, and what a person does to it:
//! keys, taps, the screen on top.

use std::rc::Rc;

use ddi_ui::dev_widgets::FoldFn;
use ddi_ui::event::KeyEvent;
use ddi_ui::screens::Env;
use ddi_ui::{Ui, UiEvent};

/// Search's folding as the window's (NFKD).
pub fn nfkd() -> FoldFn {
    Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_desktop::Nfkd, s))
}

/// Search's folding to lower case (enough where nothing is searched).
pub fn lowercase() -> FoldFn {
    Rc::new(|s: &str| s.to_lowercase())
}

/// Search's folding as it is.
pub fn identity() -> FoldFn {
    Rc::new(str::to_string)
}

/// The window's UI (`shell::theme_ui`) on `first`, its motion reduced
/// where `reduced`, sized to `size` at a ratio of 1.
pub fn theme(first: &str, fold: FoldFn, size: [u32; 2], reduced: bool) -> (Ui, Env) {
    // A practice room it opens on, seeded as every test's.
    super::practice::seeded();
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui(first, fold);
    if reduced {
        env.reduced_motion.set(true);
    }
    resize(&mut ui, size);
    (ui, env)
}

/// `ui` sized to `size` at a ratio of 1.
pub fn resize(ui: &mut Ui, size: [u32; 2]) {
    ui.handle(UiEvent::Resize {
        size: [size[0] as f32, size[1] as f32],
        ratio: 1.0,
    });
}

/// A key pressed and released.
pub fn key(ui: &mut Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

/// [`key`], then a twentieth of a second.
pub fn key_step(ui: &mut Ui, code: &str, k: &str) {
    key(ui, code, k);
    ui.update(0.05);
}

/// A key pressed and released (its code its name), then `seconds` of
/// frames at 60 a second.
pub fn key_frames(ui: &mut Ui, code: &str, seconds: f32) {
    key(ui, code, code);
    frames(ui, seconds);
}

/// `seconds` of frames at 60 a second.
pub fn frames(ui: &mut Ui, seconds: f32) {
    for _ in 0..(seconds * 60.0).round() as usize {
        ui.update(1.0 / 60.0);
    }
}

/// The layer on top's name.
pub fn top(ui: &Ui) -> String {
    ui.layers()
        .last()
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

/// A tap at the middle of the actor named `name` on `ui` (a mouse's),
/// then `seconds` of frames.
pub fn tap(ui: &mut Ui, name: &str, seconds: f32) {
    reveal(ui, name);
    let b = ui
        .bounds(name)
        .unwrap_or_else(|| panic!("no actor {name:?} with a box"));
    let v = ui.viewport();
    let ph = v.to_physical([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]);
    let pos = [ph[0] / v.ratio, ph[1] / v.ratio];
    for (phase, buttons) in [
        (ddi_ui::PointerPhase::Down, ddi_ui::event::buttons::PRIMARY),
        (ddi_ui::PointerPhase::Up, 0),
    ] {
        ui.handle(UiEvent::Pointer(ddi_ui::PointerEvent {
            id: 1,
            kind: ddi_ui::PointerKind::Mouse,
            phase,
            pos,
            buttons,
            modifiers: ddi_ui::Modifiers::default(),
            time: 0.0,
        }));
    }
    frames(ui, seconds);
}

/// Scrolls `name` into view whole where a pop-up's rows hold it scrolled
/// away (the Rules pop-up's), by the wheel over the screen's middle, as
/// a person would before tapping it.
fn reveal(ui: &mut Ui, name: &str) {
    let whole = |ui: &Ui| match (ui.bounds(name), ui.visible_bounds(name)) {
        (Some(b), Some(v)) => b.iter().zip(v).all(|(a, b)| (a - b).abs() < 0.5),
        _ => ui.bounds(name).is_none(),
    };
    for dy in [40.0, -40.0] {
        for _ in 0..200 {
            if whole(ui) {
                return;
            }
            let v = ui.viewport();
            let mid = v.to_physical([v.screen.width / 2.0, v.screen.height / 2.0]);
            ui.handle(UiEvent::Wheel(ddi_ui::event::WheelEvent {
                pos: [mid[0] / v.ratio, mid[1] / v.ratio],
                delta: [0.0, dy],
                source: ddi_ui::event::WheelSource::Pixels,
                modifiers: ddi_ui::Modifiers::default(),
                time: 0.0,
            }));
            ui.update(1.0 / 60.0);
        }
    }
}

/// What a play's results show that the play decided: the notes as the
/// footer says them, and whether Retry and the song offset's editor show.
#[derive(Debug, PartialEq)]
pub struct ResultsShown {
    pub notes: String,
    pub retry: bool,
    pub offset: bool,
}

/// The results on `ui`'s top screen ([`ResultsShown`]).
pub fn results_shown(ui: &Ui) -> ResultsShown {
    let text = |name: &str| match ui.actor(name).map(|a| &a.kind) {
        Some(ddi_actor::Kind::Text(t)) => t.text().to_string(),
        _ => String::new(),
    };
    ResultsShown {
        notes: text("Footer.Note"),
        retry: ui.actor("Retry").is_some(),
        offset: ui.actor("OffsetValue").is_some(),
    }
}
