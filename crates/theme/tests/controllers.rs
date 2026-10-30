//! Physical standard-controller edges, through the same resolver as both shells.
//! No injected focus or activation: every action must have a navigation path.
use std::cell::RefCell;
use std::rc::Rc;

use ddi_theme::Infinito;
use ddi_ui::TextMode;
use ddi_ui::screens::{self, Env, sample};
use ddi_ui::testing::Harness;

const SIZES: [[f32; 2]; 3] = [[1280.0, 720.0], [844.0, 390.0], [390.0, 844.0]];

fn setup(size: [f32; 2]) -> (Harness, Env) {
    let look = Rc::new(Infinito::new());
    let design = screens::design_for(&*look, size);
    let env = Env::new(
        look,
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    env.reduced_motion.set(true);
    (
        Harness::with_gestures(design, size, 1.0, TextMode::Mirror, false),
        env,
    )
}

fn button(h: &mut Harness, n: u16) {
    let control = format!("button:{n}");
    h.controller(&Default::default(), "Pad", true, &control, true);
    h.controller(&Default::default(), "Pad", true, &control, false);
    h.run(0.03);
}

fn choose(h: &mut Harness, name: &str) {
    h.run(0.3);
    for _ in 0..200 {
        if h.ui.focused() == Some(name) {
            let b = h.ui.bounds(name).expect("focused action has bounds");
            let vp = h.ui.viewport().screen;
            assert!(
                b[0] >= -1.0 && b[1] >= -1.0 && b[2] <= vp.width + 1.0 && b[3] <= vp.height + 1.0,
                "{name}: {b:?} in {vp:?}"
            );
            button(h, 0);
            h.run(0.3);
            return;
        }
        if h.ui.focused() == Some("NextPage") {
            button(h, 0);
        } else {
            button(h, 13);
        }
    }
    panic!(
        "cannot reach {name}; focus {:?}, layers {:?}",
        h.ui.focused(),
        h.ui.layers()
    );
}

#[test]
fn search_handoff_empty_results_and_more_have_a_direction_only_path() {
    for size in SIZES {
        let (mut h, env) = setup(size);
        h.push(sample::select_screen(&env, None));
        h.run(0.5);
        button(&mut h, 2);
        choose(&mut h, "Item.SearchIcon");
        h.sync("no-such-song-0123456789", [23, 23], None);
        button(&mut h, 0);
        assert_eq!(h.ui.focused(), Some("Wheel"));
        button(&mut h, 12);
        assert_ne!(h.ui.focused(), Some("Wheel"), "empty results at {size:?}");
        // Move along the header to its visible More button, without West.
        for _ in 0..12 {
            if h.ui.focused() == Some("More") {
                break;
            }
            button(&mut h, 15);
        }
        assert_eq!(h.ui.focused(), Some("More"), "{size:?}");
        button(&mut h, 0);
        choose(&mut h, "Item.ClearSearch");
        assert_eq!(h.ui.focused(), Some("Wheel"));
        assert!(!env.state.borrow().song.is_empty());
        button(&mut h, 2);
        choose(&mut h, "Item.SearchIcon");
        h.sync("Some", [4, 4], None);
        button(&mut h, 12);
        choose(&mut h, "Item.0");
        assert_eq!(env.state.borrow().search, "Some Things Must");
        button(&mut h, 0);
        assert_eq!(h.ui.focused(), Some("Wheel"));
    }
}

#[test]
fn results_reveal_every_control_and_shoulders_change_nothing() {
    use screens::results::{ResultsActions, ResultsScreen};
    for size in SIZES {
        let (mut h, env) = setup(size);
        let mut data = sample::results();
        if let Some(offset) = &mut data.offset {
            offset.suggested = Some(0.012);
            offset.current = 0.005;
        }
        h.push(ResultsScreen::new(
            env.clone(),
            data,
            ResultsActions {
                replay: Some(Rc::new(|_, _| {})),
                ..Default::default()
            },
        ));
        h.run(1.0);
        let mut visited = Vec::new();
        for _ in 0..30 {
            let name = h.ui.focused().unwrap().to_string();
            let b = h.ui.bounds(&name).unwrap();
            let vp = h.ui.viewport().screen;
            assert!(
                b[1] >= -1.0 && b[3] <= vp.height + 1.0,
                "{size:?} {name} {b:?}"
            );
            visited.push(name);
            button(&mut h, 15);
        }
        for name in [
            "More",
            "WatchReplay",
            "OffsetDown",
            "OffsetUp",
            "UseMean",
            "ResetOffset",
        ] {
            assert!(
                visited.iter().any(|n| n == name),
                "{size:?}: missing {name} in {visited:?}"
            );
        }
        // The panel sets' tabs went with the results' bar (the sets wait
        // for a picker): the shoulders have nothing to change here.
        let before = env.state.borrow().panels.clone();
        button(&mut h, 5);
        assert_eq!(env.state.borrow().panels, before);
        button(&mut h, 4);
        assert_eq!(env.state.borrow().panels, before);
    }
}

#[test]
fn options_more_is_reachable_and_modal_buttons_do_not_change_sections() {
    for size in SIZES {
        let (mut h, env) = setup(size);
        h.push(sample::options_screen(&env, None));
        h.run(0.5);
        let first = env.state.borrow().section.clone();
        button(&mut h, 5);
        assert_ne!(env.state.borrow().section, first);
        button(&mut h, 4);
        assert_eq!(env.state.borrow().section, first);
        button(&mut h, 12);
        assert_eq!(h.ui.focused(), Some("More"));
        button(&mut h, 0);
        button(&mut h, 5);
        button(&mut h, 2);
        assert_eq!(env.state.borrow().section, first);
        choose(&mut h, "Item.Section1");
        assert_ne!(env.state.borrow().section, first);
    }
}

#[test]
fn editor_grid_holds_numeric_forms_and_tap_tempo_are_controller_operable() {
    use ddi_ui::state::editor::Editor;
    use screens::editor::{EditorPlace, EditorScreen};
    for size in SIZES {
        let (mut h, env) = setup(size);
        let editor = Rc::new(RefCell::new(Editor::new_song(None, "Controller test")));
        h.push(EditorScreen::shared(
            env.clone(),
            editor.clone(),
            EditorPlace::default(),
        ));
        h.run(0.5);
        button(&mut h, 0);
        assert_eq!(editor.borrow().note_count(), 1);
        button(&mut h, 0);
        assert_eq!(editor.borrow().note_count(), 0);
        button(&mut h, 5);
        button(&mut h, 2);
        choose(&mut h, "Item.tool");
        choose(&mut h, "Item.tool:5");
        button(&mut h, 0);
        button(&mut h, 13);
        button(&mut h, 1);
        assert_eq!(editor.borrow().note_count(), 0, "cancel a pending hold");
        button(&mut h, 0);
        button(&mut h, 13);
        button(&mut h, 0);
        assert_eq!(editor.borrow().note_count(), 1);
        assert_eq!(editor.borrow().chart().notes[0].lane, 1);
        button(&mut h, 2);
        choose(&mut h, "Item.timing");
        choose(&mut h, "Item.seg:speed");
        assert_eq!(h.ui.layers().last().unwrap().name, "number-value");
        button(&mut h, 15); // ratio +0.01
        button(&mut h, 13);
        button(&mut h, 15); // duration +0.01
        button(&mut h, 13);
        button(&mut h, 15); // units seconds
        choose(&mut h, "Ok");
        assert!(
            env.state
                .borrow()
                .editor
                .as_ref()
                .unwrap()
                .segments
                .iter()
                .any(|s| s.contains("1.01") && s.contains("0.01"))
        );
        button(&mut h, 2);
        choose(&mut h, "Item.tap-tempo");
        for _ in 0..5 {
            button(&mut h, 0);
            h.run(0.47);
        }
        assert!(env.state.borrow().editor.as_ref().unwrap().tapped.is_some());
        choose(&mut h, "Apply");
        button(&mut h, 1);
        h.run(0.3);
        assert_eq!(h.ui.layers().last().unwrap().name, "editor");
    }
}

#[test]
fn free_text_can_be_submitted_after_ime_and_validation_can_be_cancelled() {
    use screens::editor_dialogs::ValueDialog;
    for size in SIZES {
        let (mut h, env) = setup(size);
        let result = Rc::new(RefCell::new(None));
        let sink = result.clone();
        h.push(ValueDialog::new(
            env.clone(),
            "Artist",
            "",
            "",
            Rc::new(move |text, _| {
                if text.trim().is_empty() {
                    return Err("Enter an artist".into());
                }
                *sink.borrow_mut() = Some(text.to_string());
                Ok(())
            }),
        ));
        h.run(0.3);
        button(&mut h, 0);
        assert!(h.texts().iter().any(|s| s == "Enter an artist"));
        h.sync("作曲者", [3, 3], Some([0, 3]));
        button(&mut h, 0);
        assert!(result.borrow().is_none(), "composition must commit first");
        h.sync("作曲者", [3, 3], None);
        button(&mut h, 0);
        h.run(0.3);
        assert_eq!(result.borrow().as_deref(), Some("作曲者"));
        assert!(h.ui.layers().is_empty());
        h.push(ValueDialog::new(
            env,
            "Title",
            "",
            "",
            Rc::new(|_, _| Err("Invalid title".into())),
        ));
        h.run(0.3);
        button(&mut h, 0);
        button(&mut h, 1);
        h.run(0.3);
        assert!(h.ui.layers().is_empty());
    }
}

#[test]
fn browser_picker_refusal_keeps_the_dialog_and_controller_back_works() {
    use screens::data::ImportView;
    use screens::select_dialogs::ImportDialog;
    let (mut h, env) = setup([390.0, 844.0]);
    let calls = Rc::new(RefCell::new(0));
    let sink = calls.clone();
    h.push(sample::select_screen(&env, None));
    h.run(0.3);
    h.push(ImportDialog::new(
        env,
        Rc::new(RefCell::new(ImportView::default())),
        Some(Rc::new(move |_, _| *sink.borrow_mut() += 1)),
    ));
    h.run(0.3);
    button(&mut h, 15);
    assert_eq!(h.ui.focused(), Some("PickFiles"));
    button(&mut h, 0);
    assert_eq!(*calls.borrow(), 0);
    assert!(h.texts().iter().any(|s| s.contains("keyboard key")));
    button(&mut h, 1);
    h.run(0.3);
    assert_eq!(h.ui.layers().len(), 1);
    assert_eq!(h.ui.layers()[0].name, "select");
}

#[test]
fn hints_follow_physical_input_and_keep_keyboard_art_separate() {
    let (mut h, env) = setup(SIZES[0]);
    h.push(sample::select_screen(&env, None));
    h.run(0.4);
    assert!(env.state.borrow().hints.iter().any(|s| s == "ENTER Play"));
    button(&mut h, 13);
    assert!(
        env.state
            .borrow()
            .hints
            .iter()
            .any(|s| s.contains("LB RB Chart")),
        "{:?}",
        env.state.borrow().hints
    );
    assert!(!env.state.borrow().hints.iter().any(|s| s.contains("ENTER")));
    let atlas = (1..=h.ui.texture_count() as u32)
        .filter_map(|id| h.ui.texture_source(id))
        .any(|s| format!("{s:?}").contains("controllers.png"));
    assert!(atlas);
    h.key("ArrowUp", "ArrowUp");
    assert!(!h.ui.hints().active());
    assert!(env.state.borrow().hints.iter().any(|s| s.contains("ENTER")));
    assert!(h.ui.problems().is_empty(), "{:?}", h.ui.problems());
}

#[test]
fn prompt_renders_primary_triggers_and_changes_on_override_without_rebinding() {
    use ddi_actor::Kind;
    use ddi_ui::controller_hints::{Source, Style};
    let (mut h, env) = setup(SIZES[0]);
    let settings = ddi_ui::state::settings::Settings::default();
    h.ui.set_controls(settings.controls.clone(), Default::default());
    h.pads(vec![ddi_engine::ConnectedPad {
        id: "Xbox Controller".into(),
        index: 0,
        standard: true,
        vendor_product: None,
    }]);
    h.ui.set_source(Source::Pad {
        slot: 0,
        id: "Xbox Controller".into(),
    });
    h.push(sample::prompt_screen(
        &env,
        screens::prompt::PromptStep::Ready,
    ));
    h.run(0.1);
    let bindings = || env.state.borrow().prompt.as_ref().unwrap().bindings.clone();
    assert_eq!(bindings(), ["LB", "LT", "RT", "RB"]);
    // Each controller body receives the runtime atlas, and has released/pressed frames.
    for i in 0..4 {
        let holder = h.ui.actor(&format!("BindingKey{i}")).unwrap();
        fn sprite(a: &ddi_actor::Actor) -> Option<&ddi_actor::Sprite> {
            if a.name == "Cap0"
                && let Kind::Sprite(s) = &a.kind
            {
                return Some(s);
            }
            a.children().iter().find_map(sprite)
        }
        assert_eq!(sprite(holder).unwrap().states.len(), 2);
    }
    h.ui.set_controls(
        settings.controls.clone(),
        std::collections::BTreeMap::from([("Xbox Controller".into(), Style::PlayStation)]),
    );
    h.frame(0.0);
    assert_eq!(bindings(), ["L1", "L2", "R2", "R1"]);
    h.ui.set_source(Source::Keyboard);
    h.frame(0.0);
    assert_eq!(
        bindings(),
        ["ArrowLeft", "ArrowDown", "ArrowUp", "ArrowRight"]
    );
    assert!(h.ui.problems().is_empty(), "{:?}", h.ui.problems());
}

#[test]
fn disconnect_releases_song_list_holds_even_without_a_mapped_release() {
    let (mut h, env) = setup(SIZES[0]);
    h.push(sample::select_screen(&env, None));
    h.run(0.4);
    h.controller(&Default::default(), "Xbox", true, "button:13", true);
    h.run(0.08);
    h.pads(vec![]);
    // The already pressed single tap may finish its delayed browse action.
    h.run(0.3);
    let song = env.state.borrow().song.clone();
    h.run(1.5);
    assert_eq!(env.state.borrow().song, song);
    assert!(!h.ui.hints().active());
}
