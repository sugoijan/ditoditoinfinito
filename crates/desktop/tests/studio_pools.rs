//! The announcer studio's packs as pools of takes, through the shell with
//! no GPU, no sound device and no window (`docs/plans/announcer-studio.md`,
//! "Packs as pools"): a draft in two languages, Mixed, a switchable tag
//! and a recording said for two lines, exported as a zip and used in the
//! game, read back by desktop's own announcers as the game reads them.

mod common;

use std::rc::Rc;

use ddi_desktop::library::announcers;
use ddi_desktop::library::files::ZipReader;
use ddi_desktop::studio::Studio;
use ddi_library::announcer::Line;
use ddi_noteskin::params::{Value, Values};
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::screens::studio::Act;

/// `seconds` of a 16-bit mono WAV at 48 kHz: a tone.
fn wav(seconds: f64, hz: f64) -> Vec<u8> {
    let n = (48_000.0 * seconds) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| ((i as f64 * hz * std::f64::consts::TAU / 48_000.0).sin() * 0.3) as f32)
        .collect();
    ddi_library::wav::write(&[samples], 48_000)
}

struct Test {
    shell: ddi_desktop::shell::Shell,
    studio: Rc<Studio>,
}

impl Test {
    fn settled(&mut self) {
        let began = std::time::Instant::now();
        loop {
            self.shell.offscreen_poll();
            if !self.studio.working() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            assert!(
                began.elapsed().as_secs() < 120,
                "the work never ends: {:?}",
                self.studio.status()
            );
        }
        self.studio.flush();
    }

    /// Recordings `names` (each a tone) picked for `line`.
    fn add(&mut self, dir: &std::path::Path, names: &[&str], line: Line) {
        let picked: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let p = dir.join(n);
                std::fs::write(&p, wav(0.6, 300.0 + 90.0 * i as f64)).unwrap();
                p
            })
            .collect();
        ddi_desktop::os::inject_picked(picked);
        self.studio.act(Act::Add(line));
        self.settled();
    }
}

#[test]
fn the_studio_makes_a_pack_in_two_languages_with_a_tag_and_a_shared_take() {
    common::indexes();
    ddi_desktop::audio::set_headless(true);
    let dir = std::env::temp_dir().join(format!("ddi-studio-pools-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("rec/en")).unwrap();
    std::fs::create_dir_all(dir.join("rec/ja")).unwrap();
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    storage::install(Rc::new(MemoryStore::default()));
    let songs = ddi_desktop::songs::Songs::new(Vec::new());
    let shell = ddi_desktop::shell::Shell::named_with("announcer", Some(0.0), Some(songs), true);
    let studio = shell
        .songs
        .as_ref()
        .and_then(|s| s.studio.borrow().clone())
        .expect("the studio, as asked for");
    let mut t = Test {
        shell,
        studio: studio.clone(),
    };
    let rec = dir.join("rec");
    t.add(&rec.join("en"), &["ready.wav"], Line::Ready);
    t.add(&rec.join("en"), &["awesome.wav"], Line::Combo(100));
    t.add(&rec.join("en"), &["cheer.wav", "damn.wav"], Line::Cheer);
    let id_of = |name: &str| {
        studio
            .draft()
            .takes
            .iter()
            .find(|t| t.source.ends_with(name))
            .unwrap()
            .id
    };
    let (awesome, damn) = (id_of("awesome.wav"), id_of("damn.wav"));
    // The dialogs' changes (the shared page's): English named, Japanese
    // added (and worked in), a tag on one take, one take said for two
    // lines.
    studio.edit(|d| {
        d.name = "Pool Voice".into();
        assert!(d.add_language("en") && d.add_language("ja"));
    });
    t.add(&rec.join("ja"), &["ready.wav"], Line::Ready);
    studio.act(Act::Language(0));
    studio.act(Act::Mixed);
    studio.edit(|d| {
        assert_eq!(d.toggle_tag(damn, "rude"), Some(true));
        assert!(d.share(awesome, Line::Cheer));
    });
    studio.act(Act::Switchable(0));
    let draft = studio.draft();
    assert_eq!(draft.languages, ["en", "ja"]);
    assert!(draft.mixed && draft.declares() && draft.scripted());

    // Exported: a folder a language, the shared take once, the ini and
    // the script; checked by the game's reader before it is written.
    let zip = dir.join("out/Pool Voice.zip");
    std::fs::create_dir_all(zip.parent().unwrap()).unwrap();
    ddi_desktop::os::inject_saved(Some(zip.clone()));
    studio.act(Act::Export);
    t.settled();
    assert!(zip.is_file(), "{:?}", studio.status());
    let read = ZipReader::open(&zip, &ddi_desktop::library::legacy_text).unwrap();
    let mut names: Vec<String> = read.names.iter().map(|n| n.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "Announcers/Pool Voice/announcer.ini",
            "Announcers/Pool Voice/announcer.lua",
            "Announcers/Pool Voice/en/evaluation cheer/cheer.opus",
            "Announcers/Pool Voice/en/evaluation cheer/damn.opus",
            "Announcers/Pool Voice/en/gameplay ready/ready.opus",
            "Announcers/Pool Voice/ja/gameplay ready/ready.opus",
            "Announcers/Pool Voice/shared/awesome.opus",
            "README.txt",
        ]
    );
    let text = |name: &str| {
        let mut bytes = Vec::new();
        read.extract_to(
            read.names.iter().find(|n| n.to_string().ends_with(name)).unwrap(),
            &mut bytes,
        )
        .unwrap();
        String::from_utf8(bytes).unwrap()
    };
    let lua = text("announcer.lua");
    assert!(lua.contains("[\"shared/awesome.opus\"] = { events = { \"gameplay 100 combo\", \"evaluation cheer\" }, lang = \"en\" },"), "{lua}");
    assert!(lua.contains("[\"en/evaluation cheer/damn.opus\"] = { tags = { \"rude\" } },"), "{lua}");
    assert!(lua.contains("pack:tag(\"rude\", { label = \"rude\", default = true })"), "{lua}");
    assert!(lua.contains("pack:languages({ \"en\", \"ja\" }, { mixed = true })"), "{lua}");
    assert!(text("announcer.ini").contains("Languages=en,ja\nLabels=English,日本語\nMixed=1\n"));

    // Used in the game: read in place as any announcer is, its rows and
    // what it says.
    studio.act(Act::UseInGame);
    t.settled();
    let made = announcers::made();
    assert_eq!(made.len(), 1, "{:?}", studio.status());
    let kept = &made[0].kept;
    assert_eq!(made[0].key(), "pool voice");
    assert!(kept.ini.is_some() && kept.script.is_some());
    let offered = kept.offered(&Values::new(), "en");
    assert_eq!(offered.refused, None);
    let langs: Vec<&str> = offered.languages.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(langs, ["en", "ja"]);
    assert!(offered.mixed);
    assert_eq!(offered.tags.len(), 1);
    assert_eq!(offered.tags[0].name, "rude");
    let en = kept.resolve(&Values::new(), "en");
    assert_eq!(en.lines["evaluation cheer"].len(), 3);
    assert_eq!(en.lines["gameplay 100 combo"].len(), 1);
    let ja = kept.resolve(&Values::new(), "ja");
    assert_eq!(ja.lines.keys().collect::<Vec<_>>(), ["gameplay ready"]);
    let mut off = Values::new();
    off.insert("tag.rude".into(), Value::Bool(false));
    assert_eq!(kept.resolve(&off, "en").lines["evaluation cheer"].len(), 2);
    let mut mixed = Values::new();
    mixed.insert("Language".into(), Value::Text("mixed".into()));
    assert_eq!(kept.resolve(&mixed, "en").lines["gameplay ready"].len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}
