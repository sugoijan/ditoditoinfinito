//! A song's scripted backgrounds and foregrounds readied for its plays
//! (`docs/plans/scripted-backgrounds.md`), off the event loop's way: their
//! files read and their images decoded on a thread, their Lua run on the
//! event loop (a song's VM stays on the thread it was made on) in slices
//! of `ddi_noteskin::lua::LOAD_SLICE` fuel, at most [`LUA_TURN`] a poll,
//! as the web runs them between its frames (`crates/web/src/scripts.rs`),
//! then their sounds decoded and the texts `Init` left drawn on a thread.
//! The host keeps what loaded ([`Ready`]) while no view holds it, so a
//! view made again after a lost device takes it as it was.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use ddi_library::backgrounds;
use ddi_session::backgrounds::MAX_CHANGE_SIDE;
use ddi_session::scripted::{FileKind, Scripts, Sizes, SongFiles};
use ddi_ui::state::settings::Settings;

use super::load::{Files, Loaded};
use super::view::Rgba;
use crate::audio::Sound;

/// The scripts' view.
pub type ScriptView = ddi_shell::scripted::ScriptView<Rgba>;

/// The longest the scripts' Lua runs in one poll (a slice may end past
/// it): the loop draws between polls.
pub const LUA_TURN: Duration = Duration::from_millis(6);

/// A song's scripts loaded: their view (the runtime, the images it
/// loads, the texts drawn so far) and their sounds decoded at the music's
/// rate, by file relative to the song's folder.
pub struct Ready {
    pub view: ScriptView,
    pub sounds: HashMap<String, Sound>,
}

/// What the reading thread hands over: the texts, the images' sizes and
/// the images.
struct Read {
    files: SongFiles,
    sizes: Sizes,
    images: Vec<(String, Rgba)>,
}

/// What the finishing thread hands over: the sounds and the texts drawn.
struct Finished {
    sounds: HashMap<String, Sound>,
    texts: Vec<(String, Rgba, [f32; 2])>,
}

/// The scripts' Lua load, run in slices.
type LuaLoad = Pin<Box<dyn Future<Output = Result<Option<Scripts>, String>>>>;

enum Step {
    /// Their files read on a thread.
    Reading(Receiver<Read>),
    /// Their Lua run on this thread, a slice at a time.
    Lua {
        load: LuaLoad,
        images: Vec<(String, Rgba)>,
        screen: [f64; 2],
    },
    /// Their sounds decoded and texts drawn on a thread.
    Finishing(Box<ScriptView>, Receiver<Finished>),
    Done,
}

/// A song's scripts being readied.
pub struct ScriptLoad {
    step: Step,
    /// What the finishing thread needs.
    files: Files,
    rate: u32,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    /// The song's layers' schedules and its foregrounds, as stored (the
    /// play retimes the foregrounds by its song offset).
    layers: [Vec<backgrounds::BgSegment>; backgrounds::LAYERS],
    foregrounds: Vec<backgrounds::Foreground>,
    env: Option<ddi_noteskin::lua::Env>,
}

impl ScriptLoad {
    /// Starts readying `loaded`'s scripts for plays with `settings` (told
    /// StepMania's 4:3 screen under a 4:3 frame, its 16:9 one otherwise);
    /// `None` when there are none to show. `wake` wakes the loop when a
    /// thread is done.
    pub fn start(
        loaded: &Loaded,
        settings: &Settings,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Option<ScriptLoad> {
        let e = &loaded.entry;
        if settings.bg_brightness <= 0.0 || e.bg_scripts.is_empty() {
            return None;
        }
        let dir = loaded.files.dir();
        let (pack, song) = (dir.parent().file_name(), dir.file_name());
        let songs = SongFiles::new(pack.to_string(), song.to_string(), &e.bg_scripts);
        let (tx, rx) = mpsc::channel();
        let files = loaded.files.clone();
        let w = wake.clone();
        let _ = std::thread::Builder::new()
            .name("ddi-scripts".into())
            .spawn(move || {
                if tx.send(read(&files, songs)).is_ok()
                    && let Some(w) = w
                {
                    w();
                }
            });
        let folders = backgrounds::script_folders_of(&e.bg_scripts);
        let aspect = ddi_session::scripted::frame_aspect(settings.background_frame);
        Some(ScriptLoad {
            step: Step::Reading(rx),
            files: loaded.files.clone(),
            rate: loaded.music.rate,
            wake,
            layers: super::view::schedule(&loaded.song, loaded),
            foregrounds: backgrounds::foregrounds(&loaded.song, &folders),
            env: Some(ddi_session::scripted::env(&loaded.song, aspect)),
        })
    }

    /// Whether the Lua runs: nothing wakes the loop, the host polls again
    /// shortly.
    pub fn running(&self) -> bool {
        matches!(self.step, Step::Lua { .. })
    }

    /// Moves the load on, running the Lua for at most about `turn`: once
    /// it is over, the scripts (`None`: none to show, or they did not
    /// load, noted on the log; the song background shows in their place).
    pub fn poll(&mut self, turn: Duration) -> Option<Option<Ready>> {
        loop {
            match std::mem::replace(&mut self.step, Step::Done) {
                Step::Reading(rx) => match rx.try_recv() {
                    Ok(read) => self.step = self.lua(read),
                    Err(TryRecvError::Empty) => {
                        self.step = Step::Reading(rx);
                        return None;
                    }
                    Err(TryRecvError::Disconnected) => return Some(None),
                },
                Step::Lua {
                    mut load,
                    images,
                    screen,
                } => {
                    let started = Instant::now();
                    let mut cx = Context::from_waker(Waker::noop());
                    let loaded = loop {
                        match load.as_mut().poll(&mut cx) {
                            Poll::Ready(r) => break r,
                            Poll::Pending if started.elapsed() >= turn => {
                                self.step = Step::Lua {
                                    load,
                                    images,
                                    screen,
                                };
                                return None;
                            }
                            Poll::Pending => {}
                        }
                    };
                    match loaded {
                        Ok(Some(scripts)) => {
                            for note in scripts.notes() {
                                eprintln!("scripted backgrounds: {note}");
                            }
                            let sounds = scripts.sound_files();
                            let mut view = ScriptView::default();
                            view.set(Some(scripts), screen, images);
                            // Their text as Init left it, drawn now rather
                            // than in a frame (the system's fonts are read
                            // the first time).
                            let texts = view.measure();
                            self.step = Step::Finishing(Box::new(view), self.finish(sounds, texts));
                        }
                        Ok(None) => return Some(None),
                        Err(why) => {
                            eprintln!("scripted backgrounds did not load: {why}");
                            return Some(None);
                        }
                    }
                }
                Step::Finishing(mut view, rx) => {
                    let finished = match rx.try_recv() {
                        Ok(f) => f,
                        Err(TryRecvError::Empty) => {
                            self.step = Step::Finishing(view, rx);
                            return None;
                        }
                        // Shown without them.
                        Err(TryRecvError::Disconnected) => Finished {
                            sounds: HashMap::new(),
                            texts: Vec::new(),
                        },
                    };
                    for (text, image, size) in finished.texts {
                        view.add_text(text, image, size);
                    }
                    return Some(Some(Ready {
                        view: *view,
                        sounds: finished.sounds,
                    }));
                }
                Step::Done => return Some(None),
            }
        }
    }

    /// Readies the scripts to their end on this thread (tests).
    pub fn wait(mut self) -> Option<Ready> {
        loop {
            if let Some(r) = self.poll(Duration::from_secs(60)) {
                return r;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The Lua's load over what was read, a slice each turn.
    fn lua(&mut self, read: Read) -> Step {
        let Read {
            files,
            sizes,
            images,
        } = read;
        let env = self.env.take().unwrap_or_default();
        let screen = env.screen;
        let layers = std::mem::take(&mut self.layers);
        let foregrounds = std::mem::take(&mut self.foregrounds);
        let start = Instant::now();
        let clock = Box::new(move || start.elapsed().as_secs_f64());
        let load = Box::pin(async move {
            // A turn of the host: the poll ends there.
            let mut pause = || -> Pin<Box<dyn Future<Output = ()>>> {
                let mut paused = false;
                Box::pin(std::future::poll_fn(move |_| {
                    if std::mem::replace(&mut paused, true) {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                }))
            };
            Scripts::load_sliced(
                files,
                env,
                &layers,
                &foregrounds,
                &sizes,
                clock,
                Some(&mut pause),
            )
            .await
        });
        Step::Lua {
            load,
            images,
            screen,
        }
    }

    /// Decodes `sounds` and draws `texts` on a thread.
    fn finish(&self, sounds: Vec<String>, texts: Vec<String>) -> Receiver<Finished> {
        let (tx, rx) = mpsc::channel();
        let (files, rate, wake) = (self.files.clone(), self.rate, self.wake.clone());
        let _ = std::thread::Builder::new()
            .name("ddi-scripts".into())
            .spawn(move || {
                let mut out = Finished {
                    sounds: HashMap::new(),
                    texts: Vec::new(),
                };
                // At the music's rate.
                for f in sounds {
                    let ext = f.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
                    let sound = files.read(&f).and_then(|bytes| {
                        crate::audio::decoder::sound(
                            bytes,
                            ext.as_deref(),
                            Default::default(),
                            rate,
                        )
                    });
                    match sound {
                        Ok(sound) => {
                            out.sounds.insert(f, sound);
                        }
                        Err(e) => eprintln!("scripted backgrounds: {f}: {e}"),
                    }
                }
                for text in texts {
                    if let Some((image, size)) = text_image(&text) {
                        out.texts.push((text, image, size));
                    }
                }
                if tx.send(out).is_ok()
                    && let Some(w) = wake
                {
                    w();
                }
            });
        rx
    }
}

/// A text of the scripts drawn white, as a work's layers' text is drawn,
/// and its size.
pub fn text_image(text: &str) -> Option<(Rgba, [f32; 2])> {
    let size = ddi_session::scripted::TEXT_SIZE;
    super::stage::text_image(text, size, "#ffffff").map(|(image, (w, h))| (image, [w, h]))
}

/// The scripts' files read where the library found them: the Lua and
/// the other actor files as text, the images decoded (and their own
/// sizes).
fn read(files: &Files, mut songs: SongFiles) -> Read {
    let mut sizes = Sizes::default();
    let mut images = Vec::new();
    for f in songs.files.clone() {
        match FileKind::of(&f) {
            FileKind::Text => {
                if let Ok(bytes) = files.read(&f) {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    songs.texts.push((f, text));
                }
            }
            FileKind::Image => {
                let source = files.source(&f);
                if let Some((size, data, own)) = crate::images::load_sized(&source, MAX_CHANGE_SIDE)
                {
                    sizes.insert(&f, own);
                    images.push((f, Rgba { size, data }));
                }
            }
            FileKind::Other => {}
        }
    }
    Read {
        files: songs,
        sizes,
        images,
    }
}
