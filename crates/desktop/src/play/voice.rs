//! The announcer's voice for a play on desktop (`docs/plans/announcers.md`,
//! step 4): the chosen announcer found in the song folders
//! (`crate::library::announcers`), the sounds a play of the chart
//! decodes (`ddi_session::announcer::wanted`, the bags kept for the run)
//! read and decoded on a thread, and handed to the play as they come
//! ([`VoiceLoad::poll`]). Start never waits for them. Song select's lines
//! load the same way (`For::Menu`, step 5).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, LazyLock, Mutex};

use ddi_library::announcer::Line;
use ddi_session::announcer::{Bags, For, SharedVoice};

use crate::audio::Sound;
use crate::library::announcers::DiskAnnouncer;

/// The lines' shuffles, for the run of the game.
static BAGS: LazyLock<Mutex<Bags>> = LazyLock::new(|| {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    Mutex::new(Bags::new(seed))
});

thread_local! {
    /// TESTING ONLY: this thread's own shuffles ([`seed_for_tests`]).
    static TEST_BAGS: std::cell::RefCell<Option<Arc<Mutex<Bags>>>> =
        const { std::cell::RefCell::new(None) };
}

/// TESTING ONLY: the voices this thread loads from now on take their
/// sounds from shuffles of their own, begun anew from `seed`, rather
/// than from the run's (seeded by the time, and shared by every thread):
/// so which take of each line a test's play says, and how long it
/// speaks, is the same in every run and whatever other tests load.
pub fn seed_for_tests(seed: u64) {
    TEST_BAGS.with_borrow_mut(|b| *b = Some(Arc::new(Mutex::new(Bags::new(seed)))));
}

/// What the thread sends.
enum Arrived {
    /// The lines being decoded.
    Expect(Vec<Line>),
    /// A sound of a line, from its file (its path in the announcer), and
    /// its tail (`ddi_library::announcer::tail`, measured on the thread).
    Sound(Line, String, Sound, f64),
    Failed(String),
}

/// The tail of an announcer's `sound` as decoded: the seconds after its
/// voice ends (`voice-tail-v0`, `ddi_library::announcer::tail`), which
/// the voice does not count as the line being said.
pub fn tail(sound: &Sound) -> f64 {
    ddi_library::announcer::tail::tail_interleaved(
        &sound.data,
        usize::from(sound.channels),
        sound.rate,
    )
}

/// An announcer's sounds being decoded for a play.
pub struct VoiceLoad {
    pub voice: SharedVoice<Sound>,
    rx: Receiver<Arrived>,
    /// The thread ended and all it sent was taken in.
    done: bool,
}

/// Tests: the decode thread waits this long before it begins, as a
/// loaded machine would have it ([`set_test_delay`]).
static TEST_DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Tests: every voice's decode thread started from now on begins `ms`
/// later (0: at once), so a test that waits for the voice by counting
/// instead of asking ([`VoiceLoad::done`]) fails rather than passing by
/// luck.
pub fn set_test_delay(ms: u64) {
    TEST_DELAY_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
}

impl VoiceLoad {
    /// Announcer `key`'s sounds `of` a play or song select, decoded at
    /// `rate` Hz; `wake` is called as each comes.
    pub fn start(
        key: String,
        folders: Vec<PathBuf>,
        of: For,
        rate: u32,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> VoiceLoad {
        let (tx, rx) = mpsc::channel();
        let test_bags = TEST_BAGS.with_borrow(Clone::clone);
        // The player's choices for it (its language), read here: the
        // game's language is this thread's.
        let values = ddi_ui::state::settings::Settings::cached().announcer_params(&key);
        let game = ddi_l10n::current().tag.to_string();
        let _ = std::thread::Builder::new()
            .name("ddi-announcer".into())
            .spawn(move || {
                let delay = TEST_DELAY_MS.load(std::sync::atomic::Ordering::Relaxed);
                if delay > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                }
                // Ours without walking the song folders; the folders'
                // as found since the library last scanned.
                let found: Arc<[DiskAnnouncer]> = if ddi_library::announcer::is_bundled(&key) {
                    crate::library::announcers::bundled().into()
                } else {
                    crate::library::announcers::found(&folders)
                };
                let Some(a) = found.iter().find(|a| a.key() == key) else {
                    let _ = tx.send(Arrived::Failed(format!(
                        "no announcer {key} in the song folders"
                    )));
                    return;
                };
                let bags: &Mutex<Bags> = test_bags.as_deref().unwrap_or(&BAGS);
                let (kept, refused) = a.kept.resolve_said(&values, &game);
                if let Some(why) = refused {
                    eprintln!("announcer {key}: its script stopped: {why}; its folders speak");
                }
                let wanted = match bags.lock() {
                    Ok(mut bags) => of.wanted(&kept, &mut bags),
                    Err(_) => return,
                };
                let _ = tx.send(Arrived::Expect(wanted.iter().map(|(l, _)| *l).collect()));
                let reader = match a.reader() {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(Arrived::Failed(e));
                        return;
                    }
                };
                for (line, files) in wanted {
                    for file in files {
                        let sound = reader.read(&a.name_in_reader(&file)).and_then(|bytes| {
                            crate::audio::decoder::sound(
                                bytes,
                                file.extension(),
                                Default::default(),
                                rate,
                            )
                        });
                        let sent = tx.send(match sound {
                            Ok(s) => {
                                let tail = tail(&s);
                                Arrived::Sound(line, file.to_string(), s, tail)
                            }
                            Err(e) => Arrived::Failed(format!("{file}: {e}")),
                        });
                        // The play went: no more.
                        if sent.is_err() {
                            return;
                        }
                        if let Some(w) = &wake {
                            w();
                        }
                    }
                }
            });
        VoiceLoad {
            voice: Default::default(),
            rx,
            done: false,
        }
    }

    /// Takes in what was decoded since.
    pub fn poll(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(Arrived::Expect(lines)) => self.voice.borrow_mut().expect(lines),
                Ok(Arrived::Sound(line, file, sound, tail)) => {
                    self.voice.borrow_mut().add_from(line, &file, sound, tail)
                }
                Ok(Arrived::Failed(e)) => eprintln!("announcer: {e}"),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.done = true;
                    break;
                }
            }
        }
    }

    /// Every sound that will come has come (the thread ended) and was
    /// taken in by [`VoiceLoad::poll`].
    pub fn done(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Every sound under `dir`, sorted.
    fn sounds(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                out.extend(sounds(&p));
            } else if p.extension().is_some_and(|x| x == "mp3" || x == "ogg" || x == "wav") {
                out.push(p);
            }
        }
        out.sort();
        out
    }

    /// The built-in announcer as desktop decodes it for a play: every
    /// sound of its stadium-processed lines has a tail (`voice-tail-v0`),
    /// so the next line may start over it; its opening's (ready, restart,
    /// here we go), which kept their dry length, have none, so a play's
    /// opening is placed exactly as before.
    #[test]
    fn the_built_in_announcer_s_lines_have_tails_but_its_opening() {
        let built_in = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/announcers/Clank");
        for lang in ["en", "ja"] {
            let all = sounds(&built_in.join(lang));
            assert!(all.len() > 200, "{lang}: {}", all.len());
            for path in all {
                let line = path.parent().and_then(|p| p.file_name()).unwrap().to_string_lossy();
                let bytes = std::fs::read(&path).unwrap();
                let sound =
                    crate::audio::decoder::sound(bytes, Some("mp3"), Default::default(), 48_000)
                        .unwrap();
                let tail = super::tail(&sound);
                let opening = matches!(
                    &*line,
                    "gameplay ready" | "gameplay restart" | "gameplay here we go normal"
                );
                if opening {
                    assert_eq!(tail, 0.0, "{}", path.display());
                } else {
                    assert!((0.4..1.0).contains(&tail), "{}: {tail}", path.display());
                }
            }
        }
    }

    /// The tails of the sounds under each of `DDI_VOICE_TAILS`'s folders
    /// (`:`-separated; the built-in announcer's voices where none is
    /// given) as desktop decodes them for a play (48 kHz): by folder, how
    /// many have a tail, its median and longest, those without one, and
    /// how long measuring took (this build's; a play decodes about 47).
    /// `cargo test -p ddi-desktop --lib voice::tests::tails -- --ignored --nocapture`
    #[test]
    #[ignore = "a study over a pack's sounds: DDI_VOICE_TAILS"]
    fn tails() {
        let built_in = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/announcers/Clank");
        let dirs: Vec<PathBuf> = match std::env::var("DDI_VOICE_TAILS") {
            Ok(v) => v.split(':').map(PathBuf::from).collect(),
            Err(_) => vec![built_in.join("en"), built_in.join("ja")],
        };
        for dir in dirs {
            let mut tails = Vec::new();
            let mut none = 0;
            let mut untailed = std::collections::BTreeSet::new();
            let mut took = std::time::Duration::ZERO;
            for path in sounds(&dir) {
                let bytes = std::fs::read(&path).unwrap();
                let ext = path.extension().and_then(|e| e.to_str()).map(str::to_string);
                let sound =
                    crate::audio::decoder::sound(bytes, ext.as_deref(), Default::default(), 48_000)
                        .unwrap();
                let began = std::time::Instant::now();
                let tail = super::tail(&sound);
                took += began.elapsed();
                if tail > 0.0 {
                    tails.push(tail);
                } else {
                    none += 1;
                    let line = path.parent().and_then(|p| p.file_name()).unwrap();
                    untailed.insert(line.to_string_lossy().into_owned());
                }
            }
            tails.sort_by(f64::total_cmp);
            let n = tails.len() + none;
            println!(
                "{}: {} with a tail of {} sounds; median {:.3} s, shortest {:.3}, longest {:.3}; \
                 none in {} ({:?}); measured in {:.2} ms",
                dir.display(),
                tails.len(),
                n,
                tails.get(tails.len() / 2).copied().unwrap_or(0.0),
                tails.first().copied().unwrap_or(0.0),
                tails.last().copied().unwrap_or(0.0),
                none,
                untailed,
                took.as_secs_f64() * 1000.0,
            );
        }
    }
}
