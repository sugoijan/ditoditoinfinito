//! The shared screens' wording fits the platform the shell says: on
//! desktop (song folders read in place, nothing stored to remove) no
//! screen talks of the browser, its storage or the page.

use std::rc::Rc;

use ddi_platform::storage::{self, MemoryStore};

use super::credits;
use super::data::{PackEntry, Platform};
use super::options_model::{OptionsModel, Reset, Sample, World};
use super::select_dialogs::{ImportDialog, Manage, ManageDialog};
use super::{COMPACT, DESIGN, Env, sample};
use crate::dev_widgets::FoldFn;
use crate::look::Plain;
use crate::state::settings::Settings;
use crate::testing::Harness;
use crate::{PointerKind, TextMode};

/// Words a desktop screen must not say.
const WEB_ONLY: &[&str] = &["browser", "this page", "IndexedDB", "site", "download"];

fn env() -> Env {
    let fold: FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s));
    Env::new(Rc::new(Plain), fold)
}

fn check(what: &str, text: &str) {
    let lower = text.to_lowercase();
    for w in WEB_ONLY {
        assert!(
            !lower.contains(&w.to_lowercase()),
            "{what} says “{w}” on desktop: {text}"
        );
    }
}

#[test]
fn desktop_screens_do_not_talk_of_the_browser() {
    storage::install(Rc::new(MemoryStore::default()));
    let env = env();
    for (size, design) in [([1280.0, 720.0], DESIGN), ([844.0, 390.0], COMPACT)] {
        // The import dialog.
        let mut h = Harness::new(design, size, 1.0, TextMode::Own);
        let view = super::data::ImportView {
            platform: Platform::Desktop,
            ..Default::default()
        };
        h.push(ImportDialog::new(
            env.clone(),
            Rc::new(std::cell::RefCell::new(view)),
            None,
        ));
        h.run(0.3);
        for t in h.texts() {
            check("the import dialog", &t);
        }
        assert!(h.ui.bounds("Option.copy").is_none(), "no browser options");
        // Managing a pack and a song: hidden, never deleted.
        let pack = PackEntry {
            name: "Pack".into(),
            removable: true,
            rereadable: true,
        };
        for (what, button) in [
            (
                Manage::Pack {
                    pack: pack.clone(),
                    songs: 3,
                },
                "RemovePack",
            ),
            (
                Manage::Song {
                    id: "s".into(),
                    title: "Song".into(),
                    pack: pack.clone(),
                    songs: 3,
                },
                "RemoveSong",
            ),
            (Manage::Unreadable { count: 2 }, ""),
        ] {
            let mut h = Harness::new(design, size, 1.0, TextMode::Own);
            h.push(ManageDialog::new(env.clone(), what, None, false).on(Platform::Desktop));
            h.run(0.3);
            for t in h.texts() {
                check("the manage dialog", &t);
            }
            if !button.is_empty() {
                h.tap(PointerKind::Mouse, button);
                h.run(0.3);
                let texts = h.texts();
                assert!(texts.iter().any(|t| t.starts_with("Hide")), "{texts:?}");
                for t in texts {
                    check("the manage dialog's confirmation", &t);
                }
            }
        }
    }
    // Options: every section's rows, and every confirmation.
    let world = World {
        platform: Platform::Desktop,
        pads_supported: true,
        sample: Sample::Ready,
        plays: 3,
        folders: Some(vec!["/Songs".into()]),
        noteskins: vec![("folder:x".into(), "X".into())],
        ..World::default()
    };
    let settings = Settings {
        noteskin: Some("folder:x".into()),
        ..Settings::default()
    };
    let model = OptionsModel::new(settings, world);
    let data = model.data();
    for s in &data.sections {
        for o in &s.options {
            for t in [&o.label, &o.note, &o.description]
                .into_iter()
                .chain(o.values())
            {
                check(&format!("options' {}", o.id), t);
            }
        }
    }
    for r in [
        Reset::Settings,
        Reset::Songs,
        Reset::Everything,
        Reset::History,
    ] {
        let (title, body, yes) = r.texts(&model);
        for t in [title, body, yes] {
            check(&format!("{r:?}"), &t);
        }
    }
    // The credits.
    let c = credits::Credits {
        platform: Platform::Desktop,
        ..credits::sample()
    };
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(sample::credits_screen(&env, &c));
    h.run(0.3);
    let doc = format!("{:?}", credits::doc(&c));
    for w in ["the browser", "served next to"] {
        assert!(!doc.contains(w), "the credits say “{w}” on desktop");
    }
    // Calibration's menu.
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(sample::calibration_screen(&env));
    h.run(0.3);
    for t in h.texts() {
        check("calibration's menu", &t);
    }
}

#[test]
fn the_web_keeps_its_wording() {
    let env = env();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(ImportDialog::new(env, Rc::default(), None));
    h.run(0.3);
    assert!(h.texts().iter().any(|t| t.contains("browser")));
}
