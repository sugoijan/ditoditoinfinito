//! The announcer's voice for a play on desktop (`docs/plans/announcers.md`,
//! step 4): the chosen announcer found in the song folders
//! (`crate::library::announcers`), the sounds a play of the chart
//! decodes (`ddi_session::announcer::wanted`, the bags kept for the run)
//! read and decoded on a thread, and handed to the play as they come
//! ([`VoiceLoad::poll`]). Start never waits for them. Song select's lines
//! load the same way ([`For::Menu`], step 5).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, LazyLock, Mutex};

use ddi_library::announcer::Line;
use ddi_session::announcer::{Bags, SharedVoice};

use crate::audio::Sound;
use crate::library::announcers::DiskAnnouncer;

/// The lines' shuffles, for the run of the game.
static BAGS: LazyLock<Mutex<Bags>> = LazyLock::new(|| {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    Mutex::new(Bags::new(seed))
});

/// Whose sounds: a play's (of a chart of `notes` notes), a multiplayer
/// stage's (`ddi_session::announcer::battle::wanted`), or song select's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum For {
    Play { notes: u32 },
    Stage,
    Menu,
}

/// What the thread sends.
enum Arrived {
    /// The lines being decoded.
    Expect(Vec<Line>),
    Sound(Line, Sound),
    Failed(String),
}

/// An announcer's sounds being decoded for a play.
pub struct VoiceLoad {
    pub voice: SharedVoice<Sound>,
    rx: Receiver<Arrived>,
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
        let _ = std::thread::Builder::new()
            .name("ddi-announcer".into())
            .spawn(move || {
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
                let wanted = match (BAGS.lock(), of) {
                    (Ok(mut bags), For::Play { notes }) => {
                        ddi_session::announcer::wanted(&a.kept, notes, &mut bags)
                    }
                    (Ok(mut bags), For::Stage) => {
                        ddi_session::announcer::battle::wanted(&a.kept, &mut bags)
                    }
                    (Ok(mut bags), For::Menu) => ddi_session::announcer::menu(&a.kept, &mut bags),
                    (Err(_), _) => return,
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
                            Ok(s) => Arrived::Sound(line, s),
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
        }
    }

    /// Takes in what was decoded since.
    pub fn poll(&mut self) {
        while let Ok(a) = self.rx.try_recv() {
            match a {
                Arrived::Expect(lines) => self.voice.borrow_mut().expect(lines),
                Arrived::Sound(line, sound) => self.voice.borrow_mut().add(line, sound),
                Arrived::Failed(e) => eprintln!("announcer: {e}"),
            }
        }
    }
}
