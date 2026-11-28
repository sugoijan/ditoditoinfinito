//! Options → Plugins (`super`): its rows from what the shell read, the
//! levels' rule (nothing of it at Basic but its count of switches on), its
//! rows' changes and its count's Reset handed to the shell, and Options'
//! rail holding a thirteenth section and more.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ddi_platform::storage::{self, MemoryStore};

use super::*;
use crate::look::Plain;
use crate::screen::Cx;
use crate::screens::data::Platform;
use crate::screens::options::{OptionsCall, OptionsFeed, OptionsScreen};
use crate::screens::options_levels::CHANGED;
use crate::screens::options_model::{OptionsModel, Reset, World};
use crate::screens::options_shell::{self, Leave, OptionsPlatform};
use crate::screens::{DESIGN, Env};
use crate::state::settings::Settings;
use crate::testing::Harness;
use crate::{PointerKind, TextMode};

fn plugin(id: &str, on: bool, state: State) -> PluginView {
    PluginView {
        id: id.into(),
        name: format!("Test {id}"),
        version: "1.0.0".into(),
        publisher: "Tests".into(),
        homepage: Some(format!("https://example.org/{id}")),
        can: vec![Can::Search, Can::Fetch, Can::Link],
        links: vec!["example.org/songs".into()],
        reach: Reach::Rooms,
        sha256: "ab".repeat(32),
        on,
        state,
    }
}

fn view() -> PluginsView {
    PluginsView {
        folder: "/home/plugins".into(),
        looking: false,
        this_computer: false,
        folders: Vec::new(),
        plugins: vec![
            plugin("good", false, State::Ready),
            plugin("tired", true, State::StoppedWorking),
            plugin("future", false, State::Incompatible { plugin_newer: true }),
        ],
        refused: vec![RefusedView {
            folder: "broken".into(),
            why: Refusal::Manifest("expected `=`".into()),
        }],
    }
}

fn ids(s: &OptionSection) -> Vec<&str> {
    s.options.iter().map(|o| o.id.as_str()).collect()
}

#[test]
fn every_installed_plugin_is_a_row_with_its_state_and_every_row_is_advanced() {
    let s = section(&view());
    assert_eq!(s.id, SECTION);
    assert!(!s.lead.is_empty(), "what a plugin is, said once");
    assert_eq!(
        ids(&s),
        [
            FOLDER,
            THIS_COMPUTER,
            "plugins.plugin.good",
            "plugins.page.good",
            "plugins.plugin.tired",
            "plugins.page.tired",
            "plugins.plugin.future",
            "plugins.page.future",
            "plugins.refused.0",
        ]
    );
    assert!(s.options.iter().all(|o| o.level == Level::Advanced));
    // Switches, off by default.
    for id in [THIS_COMPUTER, "plugins.plugin.good", "plugins.plugin.tired"] {
        let o = s.options.iter().find(|o| o.id == id).unwrap();
        assert!(
            matches!(&o.kind, OptionKind::Choices(c) if c.len() == 2),
            "{id}"
        );
        assert_eq!(o.default, Some(0), "{id}");
    }
    let by = |id: &str| s.options.iter().find(|o| o.id == id).unwrap();
    // Its version, publisher and the start of its digest beside it; the
    // whole digest, what it does, how far its songs go, its page.
    let good = by("plugins.plugin.good");
    assert_eq!(good.index, 0);
    assert!(
        good.note.contains("1.0.0") && good.note.contains("Tests"),
        "{}",
        good.note
    );
    assert!(good.note.contains(&"ab".repeat(SHORT_SHA / 2)));
    assert!(!good.note.contains(&"ab".repeat(32)));
    for words in [
        "Switched off",
        &"ab".repeat(32),
        "searching",
        "fetching songs",
        "example.org/songs",
        "added to online rooms, but not imported",
        "https://example.org/good",
    ] {
        assert!(
            good.description.contains(words),
            "{words}: {}",
            good.description
        );
    }
    // On, and stopped for the run.
    let tired = by("plugins.plugin.tired");
    assert_eq!(tired.index, 1);
    assert_eq!(
        tired.note,
        "Stopped working: off until the game starts again"
    );
    assert!(
        tired
            .description
            .starts_with("It stopped working three times")
    );
    // A protocol it does not speak: nothing to switch.
    let future = by("plugins.plugin.future");
    assert_eq!(future.kind, OptionKind::Actions(Vec::new()));
    assert_eq!(future.note, "Needs a newer version of the game");
    let page = by("plugins.page.good");
    assert_eq!(page.values(), ["Open its page"]);
    assert_eq!(page.note, "https://example.org/good");
    let broken = by("plugins.refused.0");
    assert_eq!(broken.label, "broken");
    assert_eq!(broken.note, "Not taken: its plugin.toml does not read");
    assert!(broken.description.contains("expected `=`"));
    // Headings over the built-in source and the plugins.
    assert_eq!(
        by(THIS_COMPUTER).heading.as_deref(),
        Some("BUILT INTO THE GAME")
    );
    assert_eq!(
        by("plugins.plugin.good").heading.as_deref(),
        Some("INSTALLED PLUGINS")
    );
    assert_eq!(plugin_of("plugins.page.good"), Some("good"));
    assert_eq!(plugin_of("plugins.plugin.tired"), Some("tired"));
    assert_eq!(plugin_of(FOLDER), None);
}

#[test]
fn the_sample_has_a_row_of_each_state() {
    let s = section(&sample_view());
    assert_eq!(s.options.len(), 2 + 6 * 2 - 1 + 4);
}

#[test]
fn an_empty_folder_says_so_and_how_to_add_one() {
    let mut v = PluginsView {
        folder: "/home/plugins".into(),
        ..PluginsView::default()
    };
    let s = section(&v);
    assert_eq!(ids(&s), [FOLDER, THIS_COMPUTER, NONE]);
    assert_eq!(s.options[0].note, "None yet");
    assert_eq!(s.options[0].values(), ["Open folder", "Look again"]);
    assert!(s.options[0].description.contains("/home/plugins"));
    assert_eq!(s.options[2].note, "No plugin is installed.");
    assert_eq!(s.options[2].heading.as_deref(), Some("INSTALLED PLUGINS"));
    assert!(
        s.lead.contains("put its folder in the plugins folder"),
        "{}",
        s.lead
    );
    assert!(s.lead.contains("your files and the network"), "{}", s.lead);
    // While the folder is read: no "none" yet.
    v.looking = true;
    let s = section(&v);
    assert_eq!(ids(&s), [FOLDER, THIS_COMPUTER]);
    assert_eq!(s.options[0].note, "Looking…");
    // This computer's folders, where it has some.
    v.this_computer = true;
    v.folders = vec!["/a".into(), "/b".into()];
    let s = section(&v);
    assert_eq!(s.options[1].index, 1);
    assert!(
        s.options[1].description.contains("/a · /b"),
        "{}",
        s.options[1].description
    );
}

fn world(view: &PluginsView) -> World {
    World {
        platform: Platform::Desktop,
        folders: Some(Vec::new()),
        sections: vec![section(view)],
        ..World::default()
    }
}

/// At Basic nothing of it shows while every switch is off; one on (a
/// plugin, or This computer) shows the section with its count alone; at
/// Advanced it is the thirteenth section, after the tree's twelve.
#[test]
fn basic_shows_only_the_count_of_switches_on() {
    storage::install(Rc::new(MemoryStore::default()));
    let mut v = view();
    v.plugins[1].on = false;
    let mut m = OptionsModel::new(Settings::default(), world(&v));
    let basic = m.data_at(Level::Basic);
    assert!(basic.sections.iter().all(|s| s.id != SECTION));
    let advanced = m.data_at(Level::Advanced);
    assert_eq!(advanced.sections.len(), 13);
    assert_eq!(advanced.sections.last().unwrap().id, SECTION);
    v.this_computer = true;
    v.plugins[1].on = true;
    m.world = world(&v);
    let basic = m.data_at(Level::Basic);
    let plugins = basic.sections.iter().find(|s| s.id == SECTION).unwrap();
    assert_eq!(plugins.options.len(), 1);
    assert_eq!(plugins.options[0].id, format!("{CHANGED}{SECTION}"));
    assert_eq!(plugins.options[0].label, "Advanced changes: 2");
    assert!(plugins.lead.is_empty(), "the lead goes with the rows");
    assert_eq!(
        m.shell_changes(SECTION),
        [
            (THIS_COMPUTER.to_string(), 0),
            ("plugins.plugin.tired".to_string(), 0)
        ]
    );
}

/// A platform that writes down its section's calls and switches as told
/// (no question: the desktop's own asks).
struct Fake {
    model: RefCell<Option<Rc<RefCell<OptionsModel>>>>,
    view: RefCell<PluginsView>,
    log: RefCell<Vec<String>>,
}

impl OptionsPlatform for Fake {
    fn sample(&self, _play: bool, _volume: f32, _cx: &mut Cx) -> bool {
        false
    }
    fn export(&self) {}
    fn remove(&self, _reset: &Reset) {}
    fn held(&self, _id: &str) -> HashSet<String> {
        HashSet::new()
    }
    fn leave(&self, _to: Leave, _cx: &mut Cx) {}
    fn section_row(&self, call: &OptionsCall, _cx: &mut Cx) {
        self.log.borrow_mut().push(format!("{call:?}"));
        if let OptionsCall::Change { id, index } = call {
            let mut v = self.view.borrow_mut();
            if id == THIS_COMPUTER {
                v.this_computer = *index == 1;
            }
            for p in &mut v.plugins {
                if *id == format!("{PLUGIN}{}", p.id) {
                    p.on = *index == 1;
                }
            }
        }
        if let Some(m) = self.model.borrow().as_ref() {
            m.borrow_mut().world.sections = vec![section(&self.view.borrow())];
        }
    }
}

struct Rig {
    h: Harness,
    env: Env,
    model: Rc<RefCell<OptionsModel>>,
    fake: Rc<Fake>,
}

fn rig(v: PluginsView, level: Level, size: [f32; 2]) -> Rig {
    storage::install(Rc::new(MemoryStore::default()));
    Settings {
        level,
        ..Settings::default()
    }
    .save();
    let fold: crate::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_string());
    let env = Env::new(Rc::new(Plain), fold);
    let model = Rc::new(RefCell::new(OptionsModel::new(Settings::load(), world(&v))));
    let fake = Rc::new(Fake {
        model: RefCell::new(Some(model.clone())),
        view: RefCell::new(v),
        log: RefCell::default(),
    });
    let feed: OptionsFeed = Rc::new(RefCell::new(None));
    let shell = options_shell::shell(env.clone(), model.clone(), feed.clone(), fake.clone());
    let data = model.borrow().data();
    let mut h = Harness::new(DESIGN, size, 1.0, TextMode::Mirror);
    h.push(OptionsScreen::new(env.clone(), data, None).with_shell(shell, feed));
    h.run(0.3);
    Rig {
        h,
        env,
        model,
        fake,
    }
}

impl Rig {
    fn section(&mut self, id: &str) {
        for _ in 0..30 {
            if self.env.state.borrow().section == id {
                return;
            }
            self.h.key("Tab", "Tab");
            self.h.run(0.05);
        }
        panic!("no section {id}");
    }

    fn row(&mut self, id: &str) {
        for key in ["ArrowDown", "ArrowUp"] {
            for _ in 0..40 {
                if self.env.state.borrow().option_id == id {
                    return;
                }
                self.h.key(key, key);
            }
        }
        panic!("no row {id}: {:?}", self.env.state.borrow().option_rows);
    }
}

#[test]
fn a_switch_and_a_button_go_to_the_shell_and_the_model_keeps_none_of_them() {
    let mut r = rig(view(), Level::Advanced, [1280.0, 720.0]);
    r.section(SECTION);
    // The first row, with the lead above it.
    assert_eq!(r.env.state.borrow().option_id, FOLDER);
    assert!(r.h.ui.bounds("Lead.Line0").is_some());
    r.h.key("ArrowRight", "ArrowRight");
    r.h.key("Enter", "Enter");
    r.h.run(0.1);
    r.row("plugins.plugin.good");
    r.h.key("ArrowRight", "ArrowRight");
    r.h.run(0.1);
    let log = r.fake.log.borrow().clone();
    assert_eq!(
        log,
        [
            format!(
                "{:?}",
                OptionsCall::Action {
                    id: FOLDER.into(),
                    button: 1
                }
            ),
            format!(
                "{:?}",
                OptionsCall::Change {
                    id: "plugins.plugin.good".into(),
                    index: 1
                }
            ),
        ]
    );
    assert_eq!(r.env.state.borrow().value, "On");
    // Nothing of it in the settings.
    assert_eq!(Settings::load(), r.model.borrow().settings);
    assert_eq!(Settings::load().level, Level::Advanced);
}

/// The count's Reset at Basic switches every switch on back off, through
/// the shell; its Show opens the section at Advanced on the first.
#[test]
fn the_counts_reset_switches_them_off_through_the_shell() {
    let mut v = view();
    v.this_computer = true;
    let mut r = rig(v, Level::Basic, [1280.0, 720.0]);
    r.section(SECTION);
    assert_eq!(
        r.env.state.borrow().option_rows,
        [format!("{CHANGED}{SECTION}")]
    );
    // Reset, then the question's Confirm.
    r.h.key("ArrowLeft", "ArrowLeft");
    r.h.key("Enter", "Enter");
    r.h.run(0.2);
    r.h.tap(PointerKind::Mouse, "Confirm");
    r.h.run(0.3);
    let log = r.fake.log.borrow().clone();
    assert!(log.contains(&format!(
        "{:?}",
        OptionsCall::Change {
            id: THIS_COMPUTER.into(),
            index: 0
        }
    )));
    assert!(log.contains(&format!(
        "{:?}",
        OptionsCall::Change {
            id: "plugins.plugin.tired".into(),
            index: 0
        }
    )));
    assert!(!r.fake.view.borrow().this_computer);
    assert!(r.fake.view.borrow().plugins.iter().all(|p| !p.on));
    // Nothing on: the section is gone at Basic.
    assert!(
        !r.env
            .state
            .borrow()
            .option_sections
            .iter()
            .any(|s| s == SECTION)
    );
}

/// Thirteen sections fit the rail above the footer at 1280×720, a little
/// closer than twelve; many more scroll it, the chosen one kept in view,
/// and a wheel moves it.
#[test]
fn the_rail_holds_a_thirteenth_section_and_scrolls_past_what_fits() {
    let mut r = rig(view(), Level::Advanced, [1280.0, 720.0]);
    let footer = r.h.ui.bounds("Footer").expect("the footer");
    let last = r.h.ui.bounds("Section12").expect("the thirteenth");
    assert!(last[3] <= footer[1], "{last:?} above {footer:?}");
    let first = r.h.ui.bounds("Section0").unwrap();
    let second = r.h.ui.bounds("Section1").unwrap();
    let pitch = second[1] - first[1];
    assert!((40.0..48.0).contains(&pitch), "{pitch}");
    // A tap on it opens it.
    r.h.tap(PointerKind::Mouse, "Section12");
    r.h.run(0.1);
    assert_eq!(r.env.state.borrow().section, SECTION);

    // Twenty-four sections: more than fit, even closer.
    let mut many = world(&view());
    for i in 0..11 {
        let mut s = section(&view());
        s.id = format!("Extra{i}");
        s.name = format!("Extra {i}");
        many.sections.push(s);
    }
    r.model.borrow_mut().world = many;
    r.fake.model.replace(None);
    let mut r2 = {
        let fold: crate::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_string());
        let env = Env::new(Rc::new(Plain), fold);
        let feed: OptionsFeed = Rc::new(RefCell::new(None));
        let shell =
            options_shell::shell(env.clone(), r.model.clone(), feed.clone(), r.fake.clone());
        let data = r.model.borrow().data();
        let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        h.push(OptionsScreen::new(env.clone(), data, None).with_shell(shell, feed));
        h.run(0.3);
        Rig {
            h,
            env,
            model: r.model.clone(),
            fake: r.fake.clone(),
        }
    };
    let sections = r2.env.state.borrow().option_sections.len();
    assert_eq!(sections, 24);
    let clip = r2.h.ui.bounds("Sections").unwrap();
    let a = r2.h.ui.bounds("Section0").unwrap();
    let b = r2.h.ui.bounds("Section1").unwrap();
    assert_eq!(b[1] - a[1], 40.0, "as close as they go");
    // The last chosen by Tab: shown in the rail, the first out of it.
    r2.section("Extra10");
    let last = r2.h.ui.bounds("Section23").unwrap();
    assert!(
        last[3] <= clip[3] + 0.5 && last[1] >= clip[1] - 0.5,
        "{last:?} {clip:?}"
    );
    let rail = r2.h.ui.bounds("SectionRail").unwrap();
    assert!(rail[1] < clip[1] - 100.0, "scrolled: {rail:?} {clip:?}");
    assert!(r2.h.ui.visible_bounds("Section23").is_some());
    // A wheel brings the top back; the chosen stays chosen.
    let at = r2.h.centre("Sections");
    r2.h.wheel_lines(at, [0.0, -40.0]);
    r2.h.run(0.1);
    let rail = r2.h.ui.bounds("SectionRail").unwrap();
    assert!((rail[1] - clip[1] - 6.0).abs() < 0.5, "{rail:?} {clip:?}");
    assert_eq!(r2.env.state.borrow().section, "Extra10");
    // Tab past the end comes back to the first, shown.
    r2.section("Extra10");
    r2.h.key("Tab", "Tab");
    r2.h.run(0.05);
    assert_eq!(r2.env.state.borrow().section, "Play");
    let rail = r2.h.ui.bounds("SectionRail").unwrap();
    assert!((rail[1] - clip[1] - 6.0).abs() < 0.5, "{rail:?} {clip:?}");
}
