//! The announcer studio's draft through the shell, with no GPU, no
//! sound device and no window (so CI runs it): what the second review
//! pass found (`studio-editor-3`, `-4`, `-5`). A draft that cannot be
//! read is kept aside and none of its recordings' files is reused; a
//! file too large to be a recording is never taken in and the decoded
//! sound kept is bounded; a studio left with work under way and opened
//! again is the same studio, on the one draft.

mod common;

use std::rc::Rc;

use ddi_desktop::studio::Studio;
use ddi_library::announcer::Line;
use ddi_library::announcer::draft::{Sound, TAKE_MAX_BYTES};
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::screens::studio::Act;

/// `seconds` of a 16-bit mono WAV at 48 kHz: a tone.
fn wav(seconds: f64) -> Vec<u8> {
    let n = (48_000.0 * seconds) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| ((i as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() * 0.3) as f32)
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
}

#[test]
fn the_studio_s_draft_is_never_lost_to_an_error_a_mistake_or_a_second_copy() {
    common::indexes();
    ddi_desktop::audio::set_headless(true);
    let dir = std::env::temp_dir().join(format!("ddi-studio-draft-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let kept = dir.join("home/data/studio");
    std::fs::create_dir_all(kept.join("take")).unwrap();
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    storage::install(Rc::new(MemoryStore::default()));

    // A draft this game cannot read, and two of its recordings.
    std::fs::write(kept.join("draft.json"), "{ \"from\": \"a newer game\"").unwrap();
    std::fs::write(kept.join("take/1"), b"the first recording").unwrap();
    std::fs::write(kept.join("take/2"), b"the second").unwrap();

    let songs = ddi_desktop::songs::Songs::new(Vec::new());
    let shell = ddi_desktop::shell::Shell::named_with("announcer", Some(0.0), Some(songs), true);
    let studio = shell
        .songs
        .as_ref()
        .and_then(|s| s.studio.borrow().clone())
        .expect("the studio, as asked for");
    let mut t = Test { shell, studio };
    let studio = t.studio.clone();
    t.settled();
    assert!(
        studio.status().contains("draft.unreadable.json"),
        "the page says where the old draft is: {:?}",
        studio.status()
    );
    assert!(studio.draft().takes.is_empty());

    // Recordings added to the new draft: files of their own.
    studio.set_decoded_cap(150_000);
    let mut picked = Vec::new();
    for i in 0..6 {
        let file = dir.join(format!("New Recording {i}.wav"));
        std::fs::write(&file, wav(0.3)).unwrap();
        picked.push(file);
    }
    // And a song picked with them by mistake: far too large for a line.
    let song = dir.join("song.wav");
    let mut large = wav(0.3);
    large.resize(TAKE_MAX_BYTES as usize + 1, 0);
    std::fs::write(&song, large).unwrap();
    picked.push(song);
    ddi_desktop::os::inject_picked(picked);
    studio.act(Act::Add(Line::Ready));
    t.settled();
    let draft = studio.draft();
    assert_eq!(draft.takes.len(), 6, "{:?}", studio.status());
    assert!(
        draft.takes.iter().all(|t| matches!(t.sound, Sound::Read(_))),
        "{:?}",
        draft.takes
    );
    assert!(
        studio.status().contains("1 was left out"),
        "{:?}",
        studio.status()
    );
    assert!(draft.takes.iter().all(|t| t.id > 2), "{:?}", draft.takes);
    assert_eq!(
        std::fs::read(kept.join("take/1")).unwrap(),
        b"the first recording",
        "the old draft's recordings are as they were"
    );
    assert_eq!(std::fs::read(kept.join("take/2")).unwrap(), b"the second");
    assert_eq!(
        std::fs::read_to_string(kept.join("draft.unreadable.json")).unwrap(),
        "{ \"from\": \"a newer game\""
    );
    let stored: ddi_library::announcer::draft::Draft =
        serde_json::from_str(&std::fs::read_to_string(kept.join("draft.json")).unwrap()).unwrap();
    assert_eq!(stored.takes.len(), 6, "the new draft is the one kept");
    // Each 0.3 s decoded is 57,600 bytes of samples: two fit the cap.
    let (sounds, bytes) = studio.decoded();
    assert!(sounds <= 2 && bytes <= 150_000, "{sounds} sounds, {bytes} bytes");
    assert!(sounds >= 1);

    // Left while more is being added, and opened again: the same studio
    // goes on (no second one reading a draft the first still writes),
    // and everything added lands in the one draft.
    let mut more = Vec::new();
    for i in 0..5 {
        let file = dir.join(format!("Later {i}.wav"));
        std::fs::write(&file, wav(0.2)).unwrap();
        more.push(file);
    }
    ddi_desktop::os::inject_picked(more);
    studio.act(Act::Add(Line::Ready));
    assert!(studio.working());
    t.shell.offscreen_key("Escape", "Escape");
    for _ in 0..40 {
        t.shell.ui.update(1.0 / 30.0);
    }
    assert!(studio.left(), "{:?}", t.shell.ui.layers());
    // No poll between leaving and opening again: the work is a thread's,
    // and a poll that found it finished would rightly let the studio go
    // (the files are small: on a fast machine they are done by now). What
    // was asked for is under way until a poll takes it, so it is kept.
    assert!(studio.working());
    let living = t
        .shell
        .songs
        .as_ref()
        .and_then(|s| s.studio.borrow().clone())
        .expect("kept while it works");
    assert!(Rc::ptr_eq(&living, &studio));
    t.shell.ui.push(living.reopened());
    assert!(!studio.left());
    t.settled();
    assert!(
        t.shell.songs.as_ref().unwrap().studio.borrow().is_some(),
        "open again: not let go once its work ends"
    );
    let draft = studio.draft();
    assert_eq!(draft.takes.len(), 11);
    let mut ids: Vec<u64> = draft.takes.iter().map(|t| t.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), 11);
    assert!(ids.iter().all(|id| kept.join(format!("take/{id}")).is_file()));
    let stored: ddi_library::announcer::draft::Draft =
        serde_json::from_str(&std::fs::read_to_string(kept.join("draft.json")).unwrap()).unwrap();
    assert_eq!(stored.takes.len(), 11);
    let _ = std::fs::remove_dir_all(&dir);
}
