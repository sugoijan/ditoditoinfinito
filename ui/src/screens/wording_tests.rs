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
            banner: None,
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

#[test]
fn danoni_tabs_are_desktop_capability_gated_and_submit_inline_parameters() {
    use super::data::ImportView;
    use super::select::SelectCall;
    use std::cell::RefCell;
    let calls = Rc::new(RefCell::new(Vec::new()));
    let seen = calls.clone();
    let shell = Rc::new(move |call, _: &mut crate::Cx| seen.borrow_mut().push(call));
    for (platform, capability) in [
        (Platform::Web, false),
        (Platform::Web, true),
        (Platform::Desktop, false),
    ] {
        let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        h.push(ImportDialog::new(
            env(),
            Rc::new(RefCell::new(ImportView {
                platform,
                danoni_urls: capability,
                ..Default::default()
            })),
            Some(shell.clone()),
        ));
        assert!(h.ui.bounds("TabDanoni").is_none());
        assert!(h.ui.bounds("DanoniUrlField").is_none());
        assert!(h.ui.bounds("PickFolder").is_some());
    }
    let view = Rc::new(RefCell::new(ImportView {
        platform: Platform::Desktop,
        danoni_urls: true,
        ..Default::default()
    }));
    let env = env();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(ImportDialog::new(env.clone(), view.clone(), Some(shell)));
    h.run(0.2);
    assert_eq!(h.ui.focused(), Some("TabFiles"));
    h.key("ArrowRight", "ArrowRight");
    h.run(0.2);
    assert_eq!(h.ui.focused(), Some("TabDanoni"));
    assert!(h.ui.bounds("PickFolder").is_none());
    assert!(h.ui.bounds("DanoniUrlField").is_some());
    h.tap(PointerKind::Mouse, "ImportDanoni");
    assert!(calls.borrow().is_empty());
    assert!(h.ui.bounds("DanoniOffline").is_none());
    h.tap(PointerKind::Mouse, "DanoniUrlField");
    let url = "  https://example.org/song/  ";
    h.sync(url, [url.len(); 2], None);
    h.key("Enter", "Enter");
    h.run(0.2);
    assert_eq!(
        &*calls.borrow(),
        &[SelectCall::ImportDanoni {
            url: "https://example.org/song/".into(),
            offline: false,
        }]
    );
    assert!(
        env.state.borrow().import_open,
        "submission keeps Add songs open"
    );
    assert!(
        h.ui.bounds("DanoniUrlField").is_some(),
        "there is no extra modal"
    );
    assert_eq!(view.borrow().danoni.url, "https://example.org/song/");
    h.key("Tab", "Tab");
    h.run(0.2);
    assert!(h.ui.bounds("PickFiles").is_some());
}

#[test]
fn danoni_progress_cancel_retry_and_close_are_separate_from_the_job() {
    use super::data::{DanoniImportView, ImportReportView, ImportView};
    use super::select::{IMPORT_CHANGED, SelectCall};
    use std::cell::RefCell;
    for cancel in [false, true] {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let seen = calls.clone();
        let shell = Rc::new(move |call, _: &mut crate::Cx| seen.borrow_mut().push(call));
        let view = Rc::new(RefCell::new(ImportView {
            platform: Platform::Desktop,
            danoni_urls: true,
            danoni_cancel: true,
            running: Some("Importing…".into()),
            danoni: DanoniImportView {
                url: "https://example.org/song".into(),
                stage: Some("Converting charts".into()),
                progress: Some((2, 5)),
                ..Default::default()
            },
            ..Default::default()
        }));
        let env = env();
        let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        h.push(ImportDialog::new(env.clone(), view.clone(), Some(shell)));
        h.run(0.2);
        assert!(
            h.texts()
                .iter()
                .any(|t| t.contains("Converting charts · 2 of 5"))
        );
        assert!(h.ui.bounds("DanoniProgress.Fill").is_some());
        h.tap(PointerKind::Mouse, "ImportDanoni");
        h.tap(PointerKind::Mouse, "TabFiles");
        h.tap(PointerKind::Mouse, "PickFolder");
        assert!(
            calls.borrow().is_empty(),
            "busy import cannot start another"
        );
        if cancel {
            h.tap(PointerKind::Mouse, "CancelDanoni");
            assert_eq!(&*calls.borrow(), &[SelectCall::CancelDanoni]);
        }
        h.tap(PointerKind::Mouse, "Close");
        h.run(0.2);
        assert_eq!(calls.borrow().len(), usize::from(cancel));
        assert!(!env.state.borrow().import_open);
        assert!(
            view.borrow().running.is_some(),
            "closing never changes the job"
        );
        view.borrow_mut().running = None;
        view.borrow_mut().danoni_cancel = false;
        view.borrow_mut().report = Some(ImportReportView {
            error: true,
            summary: "An import failed. Retry when ready.".into(),
            ..Default::default()
        });
        h.push(ImportDialog::new(env, view.clone(), None));
        h.ui.broadcast(crate::Message::new(IMPORT_CHANGED));
        h.run(0.2);
        assert!(h.texts().iter().any(|t| t == "Retry"));
        assert!(h.texts().iter().any(|t| t.contains("An import failed")));
    }
}

#[test]
fn danoni_origin_decisions_are_inline_identified_and_fit_compact_dialogs() {
    use super::data::{DanoniImportView, DanoniOriginRequest, ImportView};
    use super::select::{IMPORT_CHANGED, SelectCall};
    use crate::MenuAction;
    use std::cell::RefCell;
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], [480.0, 1040.0]),
    ] {
        for (button, allow, remember) in [
            ("OriginOnce.42", true, false),
            ("OriginRemember.42", true, true),
            ("OriginDecline.42", false, false),
        ] {
            let calls = Rc::new(RefCell::new(Vec::new()));
            let seen = calls.clone();
            let shell = Rc::new(move |call, _: &mut crate::Cx| seen.borrow_mut().push(call));
            let view = Rc::new(RefCell::new(ImportView {
                platform: Platform::Desktop,
                danoni_urls: true,
                danoni_cancel: true,
                running: Some("Checking files…".into()),
                danoni: DanoniImportView {
                    url: "https://example.org/work".into(),
                    ..Default::default()
                },
                ..Default::default()
            }));
            let mut h = Harness::new(design, size, 1.0, TextMode::Mirror);
            h.push(ImportDialog::new(env(), view.clone(), Some(shell)));
            h.run(0.2);
            view.borrow_mut().danoni.origin = Some(DanoniOriginRequest {
                request_id: 42,
                origin: "https://music.example.org".into(),
                purpose: "This work's music is stored at this address.".into(),
            });
            h.ui.broadcast(crate::Message::new(IMPORT_CHANGED));
            h.run(0.2);
            assert_eq!(h.ui.focused(), Some("OriginDecline.42"));
            assert!(
                h.ui.bounds("DanoniUrlField").is_none(),
                "request gets the content area"
            );
            for name in [
                "TabFiles",
                "TabDanoni",
                "Report",
                "OriginOnce.42",
                "OriginRemember.42",
                "OriginDecline.42",
                "CancelDanoni",
                "Close",
            ] {
                let [left, top, right, bottom] =
                    h.ui.bounds(name).unwrap_or_else(|| panic!("{name}"));
                assert!(
                    left >= 0.0 && top >= 0.0 && right <= design[0] && bottom <= design[1],
                    "{name}: {:?} at {size:?}",
                    [left, top, right, bottom]
                );
            }
            let report = h.ui.bounds("Report").unwrap();
            let actions = h.ui.bounds("OriginOnce.42").unwrap();
            assert!(report[3] < actions[1], "report must not cover decisions");
            if button == "OriginDecline.42" {
                h.pad(MenuAction::Start, true);
                h.pad(MenuAction::Start, false);
            } else {
                h.tap(PointerKind::Touch, button);
            }
            assert_eq!(
                &*calls.borrow(),
                &[SelectCall::DanoniOriginDecision {
                    request_id: 42,
                    allow,
                    remember
                }]
            );
            assert!(
                view.borrow().danoni.origin.is_none(),
                "one answer consumes the request"
            );
            assert!(h.ui.problems().is_empty(), "{:?}", h.ui.problems());
        }
    }
}

#[test]
fn danoni_url_controls_fit_tabs_and_can_be_reached_by_a_pad() {
    use super::data::ImportView;
    use crate::MenuAction;
    use std::cell::RefCell;
    for (size, design) in [
        ([1280.0, 720.0], DESIGN),
        ([844.0, 390.0], COMPACT),
        ([390.0, 844.0], [480.0, 1040.0]),
    ] {
        let mut h = Harness::new(design, size, 1.0, TextMode::Mirror);
        h.push(ImportDialog::new(
            env(),
            Rc::new(RefCell::new(ImportView {
                platform: Platform::Desktop,
                danoni_urls: true,
                ..Default::default()
            })),
            None,
        ));
        h.run(0.2);
        h.pad(MenuAction::Select, true);
        h.pad(MenuAction::Select, false);
        h.run(0.2);
        assert_eq!(h.ui.focused(), Some("TabDanoni"));
        h.pad(MenuAction::Down, true);
        h.pad(MenuAction::Down, false);
        assert_eq!(h.ui.focused(), Some("DanoniUrlField"));
        for name in [
            "TabFiles",
            "TabDanoni",
            "DanoniUrlField",
            "ImportDanoni",
            "Report",
            "Close",
        ] {
            let [left, top, right, bottom] = h.ui.bounds(name).unwrap();
            assert!(
                left >= 0.0 && top >= 0.0 && right <= design[0] && bottom <= design[1],
                "{name} at {size:?}"
            );
        }
        assert!(
            h.ui.bounds("Report").unwrap()[3] < h.ui.bounds("Close").unwrap()[1],
            "report and footer do not overlap"
        );
    }
}

#[test]
fn a_pointer_release_for_an_old_origin_cannot_answer_the_next_request() {
    use super::data::{DanoniImportView, DanoniOriginRequest, ImportView};
    use super::select::IMPORT_CHANGED;
    use crate::PointerPhase;
    use std::cell::RefCell;
    let calls = Rc::new(RefCell::new(Vec::new()));
    let seen = calls.clone();
    let view = Rc::new(RefCell::new(ImportView {
        platform: Platform::Desktop,
        danoni_urls: true,
        danoni_cancel: true,
        running: Some("Checking files…".into()),
        danoni: DanoniImportView {
            origin: Some(DanoniOriginRequest {
                request_id: 42,
                origin: "https://first.example.org".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }));
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(ImportDialog::new(
        env(),
        view.clone(),
        Some(Rc::new(move |call, _: &mut crate::Cx| {
            seen.borrow_mut().push(call)
        })),
    ));
    h.run(0.2);
    let at = h.centre("OriginRemember.42");
    h.pointer(1, PointerKind::Mouse, PointerPhase::Down, at);
    view.borrow_mut().danoni.origin = Some(DanoniOriginRequest {
        request_id: 43,
        origin: "https://second.example.org".into(),
        ..Default::default()
    });
    h.ui.broadcast(crate::Message::new(IMPORT_CHANGED));
    h.pointer(1, PointerKind::Mouse, PointerPhase::Up, at);
    h.run(0.2);
    assert!(
        calls.borrow().is_empty(),
        "the stale release must not grant the next address"
    );
    assert_eq!(view.borrow().danoni.origin.as_ref().unwrap().request_id, 43);
    assert!(h.ui.bounds("OriginRemember.42").is_none());
    assert!(h.ui.bounds("OriginRemember.43").is_some());
}

#[test]
fn danoni_saved_url_does_not_overlap_the_empty_field_hint() {
    use super::data::{DanoniImportView, ImportView};
    let url = "https://example.invalid/work/";
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(ImportDialog::new(
        env(),
        Rc::new(std::cell::RefCell::new(ImportView {
            platform: Platform::Desktop,
            danoni_urls: true,
            danoni: DanoniImportView {
                url: url.into(),
                ..Default::default()
            },
            ..Default::default()
        })),
        None,
    ));
    h.run(0.3);
    let texts = h.texts();
    assert!(texts.iter().any(|s| s == url));
    assert!(!texts.iter().any(|s| s == "https://…"));
}

#[test]
fn busy_danoni_bar_moves_without_counts_and_respects_reduced_motion() {
    use super::data::{DanoniImportView, ImportView};
    use super::select::IMPORT_CHANGED;
    use std::cell::RefCell;
    for reduced in [false, true] {
        let env = env();
        env.reduced_motion.set(reduced);
        let view = Rc::new(RefCell::new(ImportView {
            platform: Platform::Desktop,
            danoni_urls: true,
            danoni_cancel: true,
            running: Some("Reading chart data…".into()),
            danoni: DanoniImportView {
                url: "https://example.org/work".into(),
                ..Default::default()
            },
            ..Default::default()
        }));
        let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        h.push(ImportDialog::new(env, view.clone(), None));
        h.run(0.2);
        let first = h.ui.bounds("DanoniProgress.Activity").unwrap();
        h.run_on_demand(0.4);
        let next = h.ui.bounds("DanoniProgress.Activity").unwrap();
        assert_eq!(first == next, reduced);
        assert!(h.ui.bounds("DanoniOffline").is_none());
        view.borrow_mut().danoni.progress = Some((2, 5));
        h.ui.broadcast(crate::Message::new(IMPORT_CHANGED));
        h.run(0.1);
        assert!(h.ui.bounds("DanoniProgress.Fill").is_some());
        let counted = h.ui.bounds("DanoniProgress.Activity").unwrap();
        h.run_on_demand(0.3);
        assert_eq!(
            counted == h.ui.bounds("DanoniProgress.Activity").unwrap(),
            reduced
        );
        view.borrow_mut().running = None;
        h.ui.broadcast(crate::Message::new(IMPORT_CHANGED));
        h.run(0.1);
        assert!(h.ui.bounds("DanoniProgress.Activity").is_none());
    }
}
