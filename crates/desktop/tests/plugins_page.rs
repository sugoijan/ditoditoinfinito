//! Options → Plugins (`docs/plans/source-plugins.md`, stage 4) on the
//! desktop's own Options and song select, over a scratch home whose
//! plugins folder holds four: the well-behaved example
//! (`ddi_source_test_plugins::example`), one that crashes at once, one
//! whose `plugin.toml` does not read, and one with no program for this
//! computer. Each row's state; nothing started by opening the page;
//! switching one on asks first (Keep it off chosen first) and Find songs
//! follows at once, switched off it goes; three crashes say it stopped
//! working; This computer's switch; Basic's count of switches on and its
//! Reset; an empty folder; Japanese; the rail with thirteen sections.
//!
//! The plugins' processes are this test's own executable, linked under
//! their names (`main`). Every wait is for a state, never for a time.

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use ddi_ui::message::Message;
use ddi_ui::screens::options::OPTIONS_CHANGED;
use ddi_ui::screens::options_plugins as section;
use ddi_ui::screens::{DESIGN, Env};
use ddi_ui::state::settings::{Level, Settings};
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode};

const SIZE: [u32; 2] = [1280, 720];
const CRASHING: &str = "crashing";

fn main() {
    let me = std::env::current_exe().expect("this executable");
    match me.file_stem().and_then(|s| s.to_str()) {
        Some("example") => ddi_source_sdk::main(ddi_source_test_plugins::example::Example),
        Some(CRASHING) => {
            eprintln!("crashing on purpose");
            std::process::exit(3);
        }
        _ => {
            the_plugins_page_from_none_to_find_songs();
            println!("plugins_page: ok");
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn manifest(id: &str, capabilities: &str, inputs: &str) -> String {
    format!(
        "id = \"{id}\"\nname = {{ en = \"Test {id}\", ja = \"テスト {id}\" }}\npublisher = \"Tests\"\nversion = \"1.0.0\"\nhomepage = \"https://example.org/{id}\"\nprotocol = [1]\nexecutable = \"{id}\"\ncapabilities = {capabilities}\ninputs = {inputs}\n"
    )
}

/// This executable installed as plugin `id` for `platform`.
fn install(plugins: &Path, id: &str, platform: &str, capabilities: &str, inputs: &str) {
    let dir = plugins.join(id);
    let bin = dir.join("bin").join(platform);
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("{id}{}", std::env::consts::EXE_SUFFIX));
    let me = std::env::current_exe().unwrap();
    if std::fs::hard_link(&me, &exe).is_err() {
        std::fs::copy(&me, &exe).unwrap();
    }
    std::fs::write(dir.join("plugin.toml"), manifest(id, capabilities, inputs)).unwrap();
}

struct Rig {
    h: Harness,
    env: Env,
    songs: Rc<ddi_desktop::songs::Songs>,
    sound: ddi_desktop::preview::SharedPlayer,
    shared: Option<Rc<ddi_desktop::options::Shared>>,
    pictures: common::gpu::Pictures,
}

impl Rig {
    /// One frame of the window's loop as far as Options goes: its rows
    /// changed outside a call told to the screen.
    fn tick(&mut self) {
        if let Some(s) = &self.shared
            && s.take_pending()
        {
            self.h.ui.broadcast(Message::new(OPTIONS_CHANGED));
        }
        self.h.run(0.05);
    }

    fn until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let started = std::time::Instant::now();
        while !done(self) {
            assert!(
                started.elapsed().as_secs() < 60,
                "never {what}: {:?} {:?}",
                self.h.ui.layers(),
                self.state().option_rows
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            self.tick();
        }
    }

    fn state(&self) -> std::cell::Ref<'_, ddi_ui::screens::ThemeState> {
        self.env.state.borrow()
    }

    fn top(&self) -> String {
        common::ui::top(&self.h.ui)
    }

    /// Options opened over song select, as its footer opens them, on the
    /// plugins folder read (off the frame).
    fn open_options(&mut self) {
        let (screen, shared) = ddi_desktop::options::screen(
            &self.env,
            Some(self.sound.clone()),
            Some(self.songs.clone()),
        );
        *self.songs.options.borrow_mut() = Some(shared.clone());
        self.shared = Some(shared);
        self.h.push(screen);
        self.h.run(0.3);
    }

    fn close_options(&mut self) {
        self.h.key("Escape", "Escape");
        self.h.run(0.5);
        assert_eq!(self.top(), "select", "{:?}", self.h.ui.layers());
        *self.songs.options.borrow_mut() = None;
        self.shared = None;
    }

    fn section(&mut self, id: &str) {
        for _ in 0..30 {
            if self.state().section == id {
                return;
            }
            self.h.key("Tab", "Tab");
            self.h.run(0.05);
        }
        panic!("no section {id}: {:?}", self.state().option_sections);
    }

    fn row(&mut self, id: &str) {
        for key in ["ArrowDown", "ArrowUp"] {
            for _ in 0..40 {
                if self.state().option_id == id {
                    return;
                }
                self.h.key(key, key);
            }
        }
        panic!("no row {id}: {:?}", self.state().option_rows);
    }

    fn looked(&mut self) {
        let looking: String = ddi_l10n::msg::sources::plugins_folder_looking().into();
        self.until("the plugins folder read", move |r| {
            r.shared.as_ref().is_some_and(|s| {
                !s.model.borrow().data().sections.iter().any(|s| {
                    s.options
                        .iter()
                        .any(|o| o.id == section::FOLDER && o.note == looking)
                })
            })
        });
        self.h.run(0.1);
    }

    /// Row `id` of the Plugins section as the model has it now.
    fn option(&self, id: &str) -> ddi_ui::screens::data::OptionDef {
        let data = self
            .shared
            .as_ref()
            .unwrap()
            .model
            .borrow()
            .data_at(Level::Advanced);
        data.sections
            .iter()
            .flat_map(|s| s.options.iter())
            .find(|o| o.id == id)
            .cloned()
            .unwrap_or_else(|| panic!("no {id}"))
    }

    fn shot(&mut self, name: &str) {
        self.h.run(0.3);
        self.pictures.shot(&mut self.h.ui, SIZE, name);
    }

    fn shot_at(&mut self, size: [u32; 2], name: &str) {
        common::ui::resize(&mut self.h.ui, size);
        self.h.run(0.3);
        self.pictures.shot(&mut self.h.ui, size, name);
        common::ui::resize(&mut self.h.ui, SIZE);
        self.h.run(0.3);
    }

    fn offered(&self) -> Vec<String> {
        self.songs
            .sources
            .borrow()
            .as_ref()
            .map(|s| s.offered().into_iter().map(|(id, _)| id).collect())
            .unwrap_or_default()
    }
}

fn the_plugins_page_from_none_to_find_songs() {
    common::indexes();
    let dir = scratch("plugins-page");
    // SAFETY: set before any thread of this run reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    ddi_desktop::os::record_opened();
    let plugins = ddi_desktop::sources::plugins_dir().unwrap();
    let platform = ddi_source_host::installed::platform();
    install(
        &plugins,
        "example",
        &platform,
        r#"["input", "search", "filters", "browse", "metadata", "fetch"]"#,
        r#"["url"]"#,
    );
    install(
        &plugins,
        CRASHING,
        &platform,
        r#"["search", "fetch"]"#,
        "[]",
    );
    install(
        &plugins,
        "elsewhere",
        "riscv64-plan9",
        r#"["search", "fetch"]"#,
        "[]",
    );
    std::fs::create_dir_all(plugins.join("broken")).unwrap();
    std::fs::write(
        plugins.join("broken/plugin.toml"),
        "id = \"broken\"\nname = \n",
    )
    .unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    Settings {
        level: Level::Advanced,
        language: Some("en".into()),
        ..Settings::default()
    }
    .save();
    assert!(ddi_l10n::set_language("en"));
    let library = dir.join("library");
    std::fs::create_dir_all(&library).unwrap();
    // Nothing on: no sources at all, as before the page.
    let songs = ddi_desktop::songs::Songs::new(vec![library]).with_sources();
    assert!(songs.sources.borrow().is_none());
    assert!(!songs.finding());
    songs.skip_offer();
    songs.start(Arc::new(|| {}));
    if let Some(l) = songs.library.borrow_mut().as_mut() {
        l.wait();
    }
    let sound = Rc::new(RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::library::preview_song,
    )));
    let (ui, env, _) =
        ddi_desktop::shell::library_ui("select", common::ui::nfkd(), sound.clone(), &songs);
    env.reduced_motion.set(true);
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.run(0.5);
    let mut r = Rig {
        h,
        env,
        songs: songs.clone(),
        sound,
        shared: None,
        pictures: common::gpu::Pictures::new(),
    };

    // Song select's More offers no Find songs: no source is on.
    r.h.tap(PointerKind::Mouse, "More");
    r.h.run(0.4);
    assert!(r.h.ui.bounds("Item.Link.Find").is_none());
    r.h.key("Escape", "Escape");
    r.h.run(0.4);

    // Options at Advanced: Plugins the thirteenth section, the rail
    // holding it above the footer.
    r.open_options();
    r.looked();
    let sections = r.state().option_sections.clone();
    assert_eq!(sections.len(), 13, "{sections:?}");
    assert_eq!(sections.last().map(String::as_str), Some(section::SECTION));
    let footer = r.h.ui.bounds("Footer").unwrap();
    let last = r.h.ui.bounds("Section12").unwrap();
    assert!(last[3] <= footer[1], "{last:?} {footer:?}");
    r.section(section::SECTION);
    assert_eq!(r.state().option_id, section::FOLDER);
    let rows = r.state().option_rows.clone();
    assert_eq!(
        rows,
        [
            section::FOLDER,
            section::THIS_COMPUTER,
            "plugins.plugin.crashing",
            "plugins.page.crashing",
            "plugins.plugin.example",
            "plugins.page.example",
            "plugins.refused.0",
            "plugins.refused.1",
        ]
    );
    // What each says: the refused ones why, the others their version.
    assert_eq!(r.option(section::FOLDER).note, "2 plugins");
    assert_eq!(r.option("plugins.refused.0").label, "broken");
    assert_eq!(
        r.option("plugins.refused.0").note,
        "Not taken: its plugin.toml does not read"
    );
    assert_eq!(r.option("plugins.refused.1").label, "elsewhere");
    assert_eq!(
        r.option("plugins.refused.1").note,
        "Not taken: no program for this computer"
    );
    let example = r.option("plugins.plugin.example");
    assert_eq!(example.index, 0, "off until switched on");
    assert!(
        example.note.starts_with("Version 1.0.0 · Tests · "),
        "{}",
        example.note
    );
    assert!(example.description.starts_with("Switched off"));
    assert!(
        example.description.contains("browsing"),
        "{}",
        example.description
    );
    // Opening the page started nothing.
    assert!(r.songs.sources.borrow().is_none());
    r.shot("page-several-en.png");
    r.shot_at([1024, 640], "page-rail-1024x640-en.png");
    r.shot_at([1920, 600], "page-rail-1920x600-en.png");

    // Open folder: the system's file browser on the plugins folder.
    r.h.key("Enter", "Enter");
    r.h.run(0.1);
    assert_eq!(
        ddi_desktop::os::take_opened(),
        std::slice::from_ref(&plugins)
    );

    // Switched on: asked first, Keep it off chosen first.
    r.row("plugins.plugin.example");
    r.h.key("ArrowRight", "ArrowRight");
    r.h.run(0.3);
    assert_eq!(r.top(), "plugin-switch-on");
    let texts = r.h.texts().join(" ");
    assert!(texts.contains("Switch on Test example?"), "{texts}");
    assert!(texts.contains("your files and the network"), "{texts}");
    r.shot("page-ask-en.png");
    r.h.key("Enter", "Enter");
    r.h.run(0.3);
    assert_ne!(r.top(), "plugin-switch-on");
    assert_eq!(r.option("plugins.plugin.example").index, 0);
    assert!(ddi_desktop::sources::plugins::switched_on().is_empty());
    assert!(r.songs.sources.borrow().is_none());
    // Asked again, and switched on.
    r.h.key("ArrowRight", "ArrowRight");
    r.h.run(0.3);
    r.h.tap(PointerKind::Mouse, "Confirm");
    r.h.run(0.3);
    assert_eq!(r.option("plugins.plugin.example").index, 1);
    assert!(ddi_desktop::sources::plugins::switched_on().contains("example"));
    assert_eq!(r.offered(), ["example"]);
    assert!(r.songs.finding());
    assert_eq!(
        r.state().notice,
        "Test example is on. Its songs are on Find songs."
    );

    // Find songs follows at once: from song select's More, the example.
    r.close_options();
    r.h.tap(PointerKind::Mouse, "More");
    r.h.run(0.4);
    r.h.tap(PointerKind::Mouse, "Item.Link.Find");
    r.h.run(0.4);
    assert_eq!(r.top(), "find-songs");
    r.until("searched", |r| {
        r.h.texts().iter().any(|t| t.contains("Generated Song 1"))
    });
    r.h.key("Escape", "Escape");
    r.h.run(0.5);
    assert_eq!(r.top(), "select");

    // The crashing one switched on, then three crashes: stopped working,
    // off until the game starts again, its switch still on.
    r.open_options();
    r.looked();
    r.section(section::SECTION);
    r.row("plugins.plugin.crashing");
    r.h.key("ArrowRight", "ArrowRight");
    r.h.run(0.3);
    r.h.tap(PointerKind::Mouse, "Confirm");
    r.h.run(0.3);
    assert_eq!(r.offered(), ["crashing", "example"]);
    for n in 1..=3 {
        {
            let mut sources = r.songs.sources.borrow_mut();
            assert!(sources.as_mut().unwrap().host.start(CRASHING));
        }
        r.until("a crash counted", |r| {
            r.songs
                .sources
                .borrow()
                .as_ref()
                .is_some_and(|s| s.host.strikes(CRASHING) == n)
        });
    }
    r.until("off for the run", |r| {
        r.songs
            .sources
            .borrow()
            .as_ref()
            .is_some_and(|s| s.host.is_off_for_the_run(CRASHING))
    });
    // Look again: what it is now.
    r.row(section::FOLDER);
    r.h.key("ArrowRight", "ArrowRight");
    r.h.key("Enter", "Enter");
    r.h.run(0.1);
    r.looked();
    assert_eq!(r.state().notice, "The plugins folder holds 2 plugins.");
    let crashing = r.option("plugins.plugin.crashing");
    assert_eq!(crashing.index, 1);
    assert_eq!(
        crashing.note,
        "Stopped working: off until the game starts again"
    );
    assert!(
        crashing
            .description
            .starts_with("It stopped working three times")
    );
    r.row("plugins.plugin.crashing");
    r.shot("page-failed-en.png");

    // Switched off: no question, stopped, gone from Find songs.
    r.h.key("ArrowLeft", "ArrowLeft");
    r.h.run(0.2);
    r.row("plugins.plugin.example");
    r.h.key("ArrowLeft", "ArrowLeft");
    r.h.run(0.2);
    assert_ne!(r.top(), "plugin-switch-on");
    assert!(ddi_desktop::sources::plugins::switched_on().is_empty());
    assert!(r.offered().is_empty());
    assert!(!r.songs.finding());

    // This computer: its switch, no question, the same setting as
    // `--sources`.
    r.row(section::THIS_COMPUTER);
    r.h.key("ArrowRight", "ArrowRight");
    r.h.run(0.2);
    assert_ne!(r.top(), "plugin-switch-on");
    assert!(ddi_desktop::sources::Setting::load().this_computer);
    assert_eq!(r.offered(), ["this-computer"]);
    assert!(r.songs.finding());

    // At Basic: the section shows only its count of switches on.
    r.section("Interface");
    r.row("show_advanced");
    r.h.key("ArrowLeft", "ArrowLeft");
    r.h.run(0.3);
    assert_eq!(r.state().options_level, "Basic");
    r.section(section::SECTION);
    assert_eq!(
        r.state().option_rows,
        [format!("advanced.changed.{}", section::SECTION)]
    );
    r.shot("page-basic-count-en.png");
    // Its Reset, asked, switches This computer off.
    r.h.key("ArrowLeft", "ArrowLeft");
    r.h.key("Enter", "Enter");
    r.h.run(0.3);
    r.h.tap(PointerKind::Mouse, "Confirm");
    r.h.run(0.3);
    assert!(!ddi_desktop::sources::Setting::load().this_computer);
    assert!(r.offered().is_empty());
    assert!(
        !r.state()
            .option_sections
            .iter()
            .any(|s| s == section::SECTION)
    );
    // Back at Advanced.
    r.section("Interface");
    r.row("show_advanced");
    r.h.key("ArrowRight", "ArrowRight");
    r.h.run(0.3);
    r.close_options();

    // Japanese.
    assert!(ddi_l10n::set_language("ja"));
    r.h.ui.rebuild_all();
    r.h.run(0.3);
    r.open_options();
    r.looked();
    r.section(section::SECTION);
    r.row("plugins.plugin.example");
    r.shot("page-several-ja.png");
    r.h.key("ArrowRight", "ArrowRight");
    r.h.run(0.3);
    assert_eq!(r.top(), "plugin-switch-on");
    r.shot("page-ask-ja.png");
    r.h.key("Enter", "Enter");
    r.h.run(0.3);
    r.close_options();
    assert!(ddi_l10n::set_language("en"));
    r.h.ui.rebuild_all();
    r.h.run(0.3);

    // An empty folder: none, and how to add one.
    std::fs::remove_dir_all(&plugins).unwrap();
    r.open_options();
    r.looked();
    r.section(section::SECTION);
    assert_eq!(
        r.state().option_rows,
        [section::FOLDER, section::THIS_COMPUTER, section::NONE]
    );
    assert_eq!(r.option(section::FOLDER).note, "None yet");
    r.shot("page-none-en.png");
    // A phone-sized window's layout: the sections a strip of tabs.
    r.h.ui.set_design(ddi_ui::screens::COMPACT);
    r.shot_at([844, 390], "page-none-compact-en.png");
    r.h.ui.set_design(DESIGN);
    r.h.run(0.3);
    r.close_options();
    songs.shutdown();
}
