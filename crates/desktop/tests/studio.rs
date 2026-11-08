//! The announcer studio through the shell (`docs/plans/announcer-studio.md`,
//! step 5), offscreen on a fake clock with no sound device: recordings
//! picked for a line and a pack imported, kept and opened again, an
//! export read back, an announcer added to the game, and each kind of
//! scene: song select, the results, and a play in its frame. Skipped
//! where there is no GPU adapter (CI).

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu;
use ddi_desktop::library::announcers;
use ddi_desktop::library::files::ZipReader;
use ddi_desktop::studio::Studio;
use ddi_library::announcer::draft::Sound;
use ddi_library::announcer::{Comment, Grade, Line, find_announcers};
use ddi_platform::HostTime;
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::event::UiEvent;
use ddi_ui::screens::studio::Act;

const SIZE: [u32; 2] = [1280, 720];
const FRAME_NS: u64 = 1_000_000_000 / 60;

/// `seconds` of a 16-bit mono WAV tone at 48 kHz, with silence around it.
fn wav(seconds: f64) -> Vec<u8> {
    let n = (48_000.0 * seconds) as usize;
    let quiet = 4_800;
    let samples: Vec<f32> = (0..n + 2 * quiet)
        .map(|i| {
            if i < quiet || i >= n + quiet {
                0.0
            } else {
                ((i as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() * 0.25) as f32
            }
        })
        .collect();
    ddi_library::wav::write(&[samples], 48_000)
}

struct Test {
    shell: ddi_desktop::shell::Shell,
    clock: Arc<AtomicU64>,
    studio: Rc<Studio>,
}

impl Test {
    /// One turn of the loop and a frame, the clock moved on.
    fn step(&mut self) {
        self.clock.fetch_add(FRAME_NS, Ordering::SeqCst);
        self.shell.offscreen_poll();
        self.shell.offscreen_frame(
            1_000.0,
            HostTime(self.clock.load(Ordering::SeqCst) as f64 / 1e9),
            1.0 / 60.0,
            false,
        );
    }

    /// The frame now, kept as `name` under `DDI_OFFSCREEN_PPM` where that
    /// is set; its pixels (RGBA rows).
    fn picture(&mut self, name: &str) -> Vec<u8> {
        let now = HostTime(self.clock.load(Ordering::SeqCst) as f64 / 1e9);
        let rgba = self
            .shell
            .offscreen_frame(1_000.0, now, 0.0, true)
            .expect("drawn offscreen");
        if let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") {
            let mut ppm = format!("P6\n{} {}\n255\n", SIZE[0], SIZE[1]).into_bytes();
            for [r, g, b, _] in rgba.as_chunks::<4>().0 {
                ppm.extend_from_slice(&[*r, *g, *b]);
            }
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), ppm).unwrap();
        }
        rgba
    }

    /// Steps until `done`, real time passing for the threads while no
    /// play runs (a play's frames follow the fake clock alone).
    fn until(&mut self, what: &str, mut done: impl FnMut(&Test) -> bool) {
        let began = std::time::Instant::now();
        while !done(self) {
            self.step();
            if !self.shell.play.playing() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(
                began.elapsed().as_secs() < 120,
                "{what}: never; status {:?}, layers {:?}, log {:?}",
                self.studio.status(),
                self.shell.ui.layers(),
                self.shell.play.log
            );
        }
    }

    fn settled(&mut self) {
        self.until("the work to end", |t| !t.studio.working());
    }

    fn studio_on_top(&self) -> bool {
        let layers = self.shell.ui.layers();
        layers.last().is_some_and(|l| {
            l.name == ddi_ui::screens::studio::NAME && matches!(l.phase, "active" | "in")
        })
    }

    /// Hears `line`, until its scene is over and the studio shows again:
    /// what it says it heard.
    fn hear(&mut self, line: Line) -> Vec<String> {
        let before = self.studio.last_heard();
        self.studio.act(Act::Hear(line));
        self.until("the scene to end", |t| {
            t.studio.last_heard() != before && !t.shell.play.has_picture()
        });
        self.until("the studio again", Test::studio_on_top);
        let (heard, spoken, _) = self.studio.last_heard().expect("heard");
        assert_eq!(heard, line);
        spoken
    }
}

#[test]
fn the_studio_on_desktop() {
    common::indexes();
    ddi_desktop::audio::set_headless(true);
    let dir = std::env::temp_dir().join(format!("ddi-studio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    storage::install(Rc::new(MemoryStore::default()));
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    ddi_desktop::audio::set_headless_clock(Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();

    let songs = ddi_desktop::songs::Songs::new(Vec::new());
    let mut shell =
        ddi_desktop::shell::Shell::named_with("announcer", Some(0.0), Some(songs), true);
    shell.offscreen(device, queue, SIZE);
    shell.ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    let c = clock.clone();
    shell.play.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    shell.play.scene_results = std::time::Duration::from_millis(400);
    let studio = shell
        .songs
        .as_ref()
        .and_then(|s| s.studio.borrow().clone())
        .expect("the studio, as asked for");
    let c = clock.clone();
    studio.set_clock(move || c.load(Ordering::SeqCst) as f64 / 1e6 + 1.0e12);
    let mut t = Test {
        shell,
        clock,
        studio: studio.clone(),
    };
    t.until("the page", Test::studio_on_top);
    for _ in 0..30 {
        t.step();
    }
    t.picture("studio-empty");

    // Recordings picked for a line: on it, decoded, measured and kept.
    let ready = dir.join("New Recording 12.wav");
    let again = dir.join("New Recording 13.wav");
    let note = dir.join("notes.txt");
    std::fs::write(&ready, wav(0.3)).unwrap();
    std::fs::write(&again, wav(0.4)).unwrap();
    std::fs::write(&note, "no sound").unwrap();
    ddi_desktop::os::inject_picked(vec![ready.clone(), again.clone(), note.clone()]);
    studio.act(Act::Add(Line::Ready));
    t.settled();
    let draft = studio.draft();
    assert_eq!(draft.takes.len(), 2, "{:?}", studio.status());
    assert!(draft.takes.iter().all(|t| t.line == Some(Line::Ready)));
    assert!(
        draft
            .takes
            .iter()
            .all(|t| matches!(t.sound, Sound::Read(_))),
        "{:?}",
        draft.takes
    );
    // The silence around it is cut as it will be in the pack.
    let tidied = draft.takes[0].measure().unwrap().tidied_seconds();
    assert!(tidied < 0.45, "{tidied}");

    // A pack made before, from a zip: on its lines, its name and credits.
    let mut files: Vec<(String, Vec<u8>)> = [
        "Voice/gameplay here we go normal/go.wav",
        "Voice/gameplay restart/r.wav",
        "Voice/gameplay failed/f.wav",
        "Voice/gameplay cleared/c.wav",
        "Voice/gameplay 100 combo/h.wav",
        "Voice/gameplay comment good/g.wav",
        "Voice/select music intro/i.wav",
        "Voice/select music comment hard/h.wav",
        "Voice/evaluation aa/aa.wav",
        "Voice/evaluation cheer/c.wav",
        "Voice/evaluation e/e.wav",
    ]
    .iter()
    .map(|p| (p.to_string(), wav(0.3)))
    .collect();
    files.push((
        "README.txt".into(),
        b"Voice\nBy A Maker\nLicense: CC0\n".to_vec(),
    ));
    let pack = dir.join("Voice.zip");
    std::fs::write(&pack, ddi_library::zip::write_stored(&files).unwrap()).unwrap();
    ddi_desktop::os::inject_picked(vec![pack]);
    studio.act(Act::ImportFiles);
    t.settled();
    let draft = studio.draft();
    assert_eq!(draft.takes.len(), 13, "{:?}", studio.status());
    assert_eq!(draft.name, "Voice");
    assert_eq!(draft.author, "A Maker");
    assert_eq!(draft.license, "CC0");
    assert_eq!(draft.takes_of(Line::Failed).count(), 1);

    // Kept: opened again as it was, its files there.
    studio.flush();
    {
        let (env, _) = t.shell.theme.clone().unwrap();
        let again = Studio::new(&env, t.shell.sound.clone(), Rc::default(), None);
        assert_eq!(again.draft(), draft);
    }

    // A recording heard, then ended by itself.
    let id = draft.takes[0].id;
    studio.act(Act::Play(id));
    assert_eq!(studio.playing(), Some(format!("take:{id}")));
    t.until("the take to end", |t| t.studio.playing().is_none());

    // Export, through the save dialog.
    let zip = dir.join("out").join("Voice.zip");
    std::fs::create_dir_all(zip.parent().unwrap()).unwrap();
    ddi_desktop::os::inject_saved(None);
    studio.act(Act::Export);
    t.settled();
    assert!(!zip.exists(), "cancelled");
    ddi_desktop::os::inject_saved(Some(zip.clone()));
    studio.act(Act::Export);
    t.settled();
    assert!(zip.is_file(), "{:?}", studio.status());
    assert!(
        studio.status().contains("Voice.zip"),
        "{:?}",
        studio.status()
    );
    let read = ZipReader::open(&zip, &ddi_desktop::library::legacy_text).unwrap();
    let listed: Vec<(String, u64)> = read
        .names
        .iter()
        .map(|n| (n.to_string(), read.size(n).unwrap_or(0)))
        .collect();
    let found = find_announcers(&listed, "x");
    assert_eq!(found.announcers.len(), 1);
    let kept = found.announcers[0].kept();
    assert_eq!(kept.name, "Voice");
    assert_eq!(kept.lines.len(), 12, "{:?}", kept.lines);

    // Use in the game: listed beside the song folders', played from there.
    assert!(announcers::made().is_empty());
    studio.act(Act::UseInGame);
    t.settled();
    let made = announcers::made();
    assert_eq!(made.len(), 1, "{:?}", studio.status());
    assert_eq!(made[0].key(), "voice");
    assert_eq!(made[0].kept.lines.len(), 12);
    assert!(
        announcers::all(&[])
            .iter()
            .any(|a| a.key() == "voice" && !a.kept.bundled)
    );
    assert!(announcers::remove_made("nothing").is_ok_and(|r| !r));

    for _ in 0..30 {
        t.step();
    }
    t.picture("studio-draft");

    // Song select's scene, by the draft.
    let line = Line::SelectMusic(Comment::Hard);
    let said = t.hear(line);
    assert_eq!(said, ["select music intro", "select music comment hard"]);
    assert!(t.shell.sound.borrow().spoken.len() >= 2);

    // The results' scene, by the draft: its line and the cheer.
    let said = t.hear(Line::Grade(Grade::Aa));
    assert_eq!(said, ["evaluation aa", "evaluation cheer"]);

    // A line not recorded: our pack's example says it.
    let said = t.hear(Line::Grade(Grade::B));
    assert_eq!(said, ["evaluation b"]);
    assert!(studio.last_heard().unwrap().2.starts_with("example:"));

    // A line said in play: the real play in its frame.
    let said = t.hear(Line::Combo(100));
    assert!(
        said.iter().any(|f| f == "gameplay 100 combo"),
        "{said:?} {:?}",
        t.shell.play.log
    );

    // The restart: the play again from the same place, opened by its line.
    let said = t.hear(Line::Restart);
    assert!(
        t.shell.play.log.iter().any(|l| l.starts_with("restarted")),
        "{:?}",
        t.shell.play.log
    );
    assert!(said.iter().any(|f| f == "gameplay restart"), "{said:?}");

    // One that ends in the results: its line, then theirs.
    let said = t.hear(Line::Failed);
    assert_eq!(said, ["gameplay failed", "evaluation e"]);

    // Left by the frame's way back, as it plays.
    let before = studio.last_heard();
    studio.act(Act::Hear(Line::CommentGood));
    t.until("the play", |t| t.shell.play.framed());
    assert!(t.shell.play_mode_on());
    for _ in 0..60 {
        t.step();
    }
    assert!(t.shell.play.has_picture());
    // The play shows, in its frame: its picture is not blank, and the
    // frame's ground around it is.
    let rgba = t.picture("studio-play");
    let lit = |x: u32, y: u32, w: u32, h: u32| -> usize {
        let mut n = 0;
        for yy in y..y + h {
            for xx in x..x + w {
                let i = ((yy * SIZE[0] + xx) * 4) as usize;
                if rgba[i] as u32 + rgba[i + 1] as u32 + rgba[i + 2] as u32 > 90 {
                    n += 1;
                }
            }
        }
        n
    };
    assert!(lit(320, 180, 640, 360) > 1000, "nothing in the picture");
    t.shell.play.back_from_scene();
    t.until("the studio again", |t| {
        t.studio.last_heard() != before && t.studio_on_top()
    });
    assert!(!t.shell.play.has_picture());
    assert_eq!(t.studio.last_heard().unwrap().0, Line::CommentGood);

    // Nothing of the scenes is kept.
    assert!(t.shell.play.log.iter().all(|l| !l.contains("history")));
    // Removing it is the way back to none.
    assert!(announcers::remove_made("voice").is_ok_and(|r| r));
    assert!(announcers::made().is_empty());

    ddi_desktop::audio::set_headless_clock(None);
    let _ = std::fs::remove_dir_all(&dir);
}
