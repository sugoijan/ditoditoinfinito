//! The bind flow on desktop's options, driven by a fake controller
//! through the same thread, hub and routing as gilrs's events
//! (`ddi_desktop::pads`, `ddi_desktop::input`): the pad moves through the
//! rows as a menu, opens its own row's bind flow with A, is then handed
//! to the dialog as it is, arms Capture and binds its d-pad and Start; the bindings
//! are saved and the menus read them at once.

use std::rc::Rc;
use std::time::Duration;

use ddi_desktop::input::Input;
use ddi_desktop::pads::{PadEvent, Pads};
use ddi_platform::storage::{self, Area, MemoryStore};
use ddi_ui::TextMode;
use ddi_ui::dev_widgets::FoldFn;
use ddi_ui::look::Plain;
use ddi_ui::screens::{DESIGN, Env};
use ddi_ui::state::settings::{KEY, Settings};
use ddi_ui::testing::Harness;

/// Hands the controllers' news to the UI until `n` events came (the
/// thread takes a moment).
fn pump(input: &mut Input, h: &mut Harness, n: usize) -> usize {
    let mut got = 0;
    for _ in 0..400 {
        for edge in input.poll(&mut h.ui) {
            // The UI knows the pad before its edges.
            assert!(!h.ui.hints().pads().is_empty());
            let e = input.route(&h.ui, edge);
            h.send(e);
            got += 1;
        }
        if got >= n {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    h.run(0.05);
    got
}

#[test]
fn a_fake_pad_binds_itself_on_the_options() {
    storage::install(Rc::new(MemoryStore::default()));
    let fold: FoldFn = Rc::new(|s: &str| s.to_string());
    let env = Env::new(Rc::new(Plain), fold);
    let state = env.state.clone();
    let (screen, shared) = ddi_desktop::options::screen(&env, None, None);
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(screen);
    h.run(0.3);
    let (pads, fake) = Pads::fake(Box::new(|| {}));
    let mut input = Input::default();
    input.pads = Some(pads);
    input.options = Some(shared.clone());
    fake.send(PadEvent::Connected {
        pad: 0,
        name: "Fake Pad".into(),
        vendor: Some(0x1234),
        product: Some(0x5678),
        standard: true,
    });
    for _ in 0..400 {
        pump(&mut input, &mut h, 0);
        if !shared.model.borrow().world.connected.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let id = "Fake Pad (Vendor: 1234 Product: 5678)";
    assert_eq!(shared.model.borrow().world.connected.len(), 1);
    let press = |fake: &ddi_desktop::pads::FakePad, index: u16| {
        fake.send(PadEvent::Button {
            pad: 0,
            index,
            pressed: true,
        });
        fake.send(PadEvent::Button {
            pad: 0,
            index,
            pressed: false,
        });
    };
    // Shoulders switch sections, then d-pad moves to this controller's row.
    for _ in 0..12 {
        if state.borrow().section == "Controls" {
            break;
        }
        press(&fake, 5);
        pump(&mut input, &mut h, 2);
    }
    assert_eq!(state.borrow().section, "Controls");
    for _ in 0..40 {
        if state.borrow().option_id == "pad.0" {
            break;
        }
        press(&fake, 13);
        pump(&mut input, &mut h, 2);
    }
    assert_eq!(
        state.borrow().option_id,
        "pad.0",
        "{:?}",
        state.borrow().option_rows
    );
    assert!(input.menu_edges > 0);
    // A opens the bind flow: from now on the pad's presses are the
    // dialog's, as they are.
    press(&fake, 0);
    pump(&mut input, &mut h, 2);
    h.run(0.3);
    assert!(h.ui.wants_raw(), "the bind dialog takes the pad");
    // Left, Down, Up, Right, then Start and Back.
    for b in [14, 13, 12, 15, 9, 8] {
        assert_eq!(h.ui.focused(), Some("Capture"));
        press(&fake, 0);
        pump(&mut input, &mut h, 2);
        press(&fake, b);
        pump(&mut input, &mut h, 2);
    }
    assert!(input.raw_edges >= 24, "{}", input.raw_edges);
    assert!(
        state.borrow().status.starts_with("bind review"),
        "{}",
        state.borrow().status
    );
    // Its new Start saves.
    fake.send(PadEvent::Button {
        pad: 0,
        index: 9,
        pressed: true,
    });
    pump(&mut input, &mut h, 1);
    h.run(0.3);
    assert!(!h.ui.wants_raw());
    let stored: Settings = storage::load(Area::Local, KEY).unwrap_or_default();
    let pad = stored.controls.pad(id, true).expect("saved");
    assert_eq!(pad.start.as_deref(), Some("button:9"));
    assert_eq!(
        pad.table("dance-single").unwrap()[0],
        vec!["button:14".to_string()]
    );
}
