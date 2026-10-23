//! The chart editor on desktop (`docs/plans/editor.md`): a song of the
//! library opened from song select (E) on the UI core's editor screen
//! (`ddi_ui::screens::editor`), saved back into its folder, or a copy
//! saved where the player picks (the system's dialog); its playtests run
//! through the play host (`crate::play::host`) on the song as edited,
//! never kept in the history.
//!
//! Saving writes `.ssc` (`ddi_chart::formats::sm_write`): over the
//! `.ssc` the song was read from, the file as it was kept beside it as
//! `<name>.old` the first time in an editing session (the screen asks
//! first); for a song read from `.sm` or `.dwi`, a new `.ssc` beside it,
//! which the library reads before them as StepMania does (the old file
//! is left as it is). Written whole (`crate::paths::write_whole`), so a
//! failed write leaves the old file whole. A bundled song or
//! one read from a zip is not saved in place: a copy only. The song's
//! pack is read again after a save. The autosave
//! (`ddi_ui::state::editor::autosave`) is `editor-autosave.json` in the
//! data folder: opening its song again recovers it.
//!
//! A new song from a music file (`new_song`) gets a folder when it is
//! made; left before its first save, the folder is removed again.

pub mod new_song;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, mpsc};

use ddi_chart::Song;
use ddi_chart::formats::sm_write;
use ddi_library::pack::ChartFormat;
use ddi_library::path::RelPathBuf;
use ddi_session::listen::{Cues, Listener};
use ddi_ui::Cx;
use ddi_ui::message::{Message, Param};
use ddi_ui::screens::Env;
use ddi_ui::screens::editor::{
    EditorCall, EditorHost, EditorPlace, EditorScreen, RecordInput, SAVED,
};
use ddi_ui::screens::editor_field::{FieldPicture, SkinField};
use ddi_ui::screens::select::{FEED, SelectCall};
use ddi_ui::state::editor::audio::{Analysis, Builder};
use ddi_ui::state::editor::autosave::Autosave;
use ddi_ui::state::editor::{Editor, export};

use crate::audio::{Audio, Sound};
use crate::play::load::Loading;

use crate::songs::Songs;

/// Where a song is saved in place.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// The `.ssc` written.
    pub file: PathBuf,
    /// It is the file the song was read from (replaced, kept as `.old`).
    pub overwrites: bool,
    /// The pack read again after a save.
    pub pack: String,
}

/// Where song `id` would be saved: `None` for a bundled song, a zip's,
/// or one that is not a simfile.
pub fn target(id: &str) -> Option<Target> {
    let s = crate::library::song(id)?;
    if s.zip || s.work.is_some() {
        return None;
    }
    let chart = &s.entry.chart;
    let ssc = ChartFormat::of(chart.as_str())? == ChartFormat::Ssc;
    let file = if ssc {
        s.dir.join(chart.as_str())
    } else {
        s.dir
            .join(chart.parent().as_str())
            .join(format!("{}.ssc", chart.file_stem()))
    };
    let file = file.to_path(&s.root)?;
    Some(Target {
        overwrites: ssc || file.exists(),
        file,
        pack: s.pack.clone(),
    })
}

/// Song `id` as the editor opens it: its simfile read, at chart `chart`.
pub fn read(id: &str, chart: usize) -> Result<Editor, String> {
    let (files, entry) = crate::play::load::Files::of(id).ok_or_else(|| format!("no song {id}"))?;
    if crate::library::song(id).is_some_and(|s| s.work.is_some())
        || ChartFormat::of(entry.chart.as_str()).is_none()
    {
        return Err("Only StepMania and DWI songs can be edited.".into());
    }
    Ok(Editor::open(
        crate::play::load::chart_of(&files, &entry)?,
        chart,
    ))
}

/// Writes `song` as `.ssc` to `file` whole; the file there before kept
/// as `<name>.old` when `backup`.
pub fn save(song: &Song, file: &Path, backup: bool) -> Result<(), String> {
    let text = sm_write::write_ssc(song);
    let name = file
        .file_name()
        .ok_or("no file name")?
        .to_string_lossy()
        .into_owned();
    if backup && file.exists() {
        std::fs::copy(file, file.with_file_name(format!("{name}.old")))
            .map_err(|e| format!("keeping the old file: {e}"))?;
    }
    crate::paths::write_whole(file, text.as_bytes()).map_err(|e| e.to_string())
}

/// Where the autosave is kept.
fn autosave_path() -> Option<PathBuf> {
    Some(crate::paths::data_dir()?.join("editor-autosave.json"))
}

/// The autosave of song `id`, if the last song edited was it.
pub fn autosaved(id: &str) -> Option<Autosave> {
    let text = std::fs::read_to_string(autosave_path()?).ok()?;
    serde_json::from_str::<Autosave>(&text)
        .ok()
        .filter(|a| a.song_id == id)
}

fn forget_autosave() {
    if let Some(p) = autosave_path() {
        let _ = std::fs::remove_file(p);
    }
}

/// The editor's calls on desktop.
struct Host {
    songs: Rc<Songs>,
    id: String,
    target: Option<Target>,
    /// The `.old` was made this session.
    backed_up: Cell<bool>,
    listen: RefCell<Listen>,
    /// A new song's folder, until its first save.
    fresh: RefCell<Option<PathBuf>>,
    /// The clock [`EditorHost::clock`] counts from.
    started: std::time::Instant,
}

/// Listening: the editor's own output, the music of a chart (decoded on
/// a thread from when the editor opens, at the output's rate; again only
/// for another file), what plays.
struct Listen {
    audio: Audio,
    id: String,
    /// The music file being read.
    loading: Option<(RelPathBuf, Loading)>,
    listener: Listener<Audio>,
    /// The music's analysis (the overview, the audio view's waveform
    /// and spectrogram), once made from it on a thread.
    analysis: Option<Rc<Analysis>>,
    making: Option<mpsc::Receiver<Analysis>>,
}

impl Listen {
    fn new(id: &str, chart: usize) -> Listen {
        let mut l = Listen {
            audio: Audio::new(1.0),
            id: id.to_string(),
            loading: None,
            listener: Listener::default(),
            analysis: None,
            making: None,
        };
        l.load(chart);
        l
    }

    /// Reads chart `chart`'s music, unless that file is read already.
    fn load(&mut self, chart: usize) {
        let file = crate::library::song(&self.id)
            .map_or_else(RelPathBuf::default, |s| s.entry.music_for(chart).to_owned());
        if self.listener.read(&file) {
            let rate = self.audio.sample_rate();
            let loading = Loading::start(&self.id, Some(chart), rate, false, None, None);
            self.loading = Some((file, loading));
            (self.analysis, self.making) = (None, None);
        }
    }

    /// The analysis of `music` (the very samples listening plays, its
    /// first at second 0), made whole on a thread.
    fn analyze(&mut self, music: Sound) {
        let (tx, rx) = mpsc::channel();
        let made = std::thread::Builder::new()
            .name("ddi-editor-audio".into())
            .spawn(move || {
                let mut b = Builder::new(f64::from(music.rate), music.frames(), 0.0);
                b.add_interleaved(&music.data, usize::from(music.channels));
                let _ = tx.send(b.finish());
            });
        self.making = made.is_ok().then_some(rx);
    }

    /// The analysis, once made.
    fn analysis(&mut self) -> Option<Rc<Analysis>> {
        self.poll();
        if let Some(rx) = &self.making {
            match rx.try_recv() {
                Ok(a) => (self.analysis, self.making) = (Some(Rc::new(a)), None),
                Err(mpsc::TryRecvError::Disconnected) => self.making = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.analysis.clone()
    }

    /// Takes in the music once read; plays it if it was waited for.
    fn poll(&mut self) {
        let Some(result) = self.loading.as_ref().and_then(|(_, l)| l.poll()) else {
            return;
        };
        let Some((file, _)) = self.loading.take() else {
            return;
        };
        match result {
            Ok(loaded) => {
                self.analyze(loaded.music.clone());
                if let Err(e) =
                    self.listener
                        .music_ready(&file, loaded.music, Some(&mut self.audio))
                {
                    eprintln!("editor: listening: {e}");
                }
            }
            Err(e) => {
                eprintln!("editor: the music: {e}");
                self.listener.music_failed(Some(&file));
            }
        }
    }

    fn ask(&mut self, from: f64) {
        let s = crate::store::settings_cached();
        // The output's offset, as a play on it takes.
        let offset = s.audio_offset_for(self.audio.fingerprint().as_ref());
        if let Err(e) = self.listener.ask(&mut self.audio, from, s.volume, offset) {
            eprintln!("editor: listening: {e}");
        }
    }

    fn stop(&mut self) {
        self.listener.stop(&mut self.audio);
    }
}

impl EditorHost for Host {
    fn call(&self, call: EditorCall, editor: &Editor, cx: &mut Cx) {
        match call {
            EditorCall::Save { revision } => {
                let message = match &self.target {
                    None => Message::new(SAVED).with(
                        "Error",
                        Param::Str("this song cannot be saved in place".into()),
                    ),
                    Some(t) => {
                        let backup = t.overwrites && !self.backed_up.get();
                        match save(editor.song(), &t.file, backup) {
                            Ok(()) => {
                                self.backed_up.set(self.backed_up.get() || backup);
                                self.fresh.borrow_mut().take();
                                forget_autosave();
                                // The song list reads the pack again.
                                self.songs
                                    .call(SelectCall::Reread(t.pack.clone()), None, cx);
                                self.songs.feed.borrow_mut().data = Some(self.songs.data());
                                cx.broadcast(Message::new(FEED));
                                Message::new(SAVED)
                                    .with("Revision", Param::Int(revision.map_or(-1, |r| r as i64)))
                            }
                            Err(e) => Message::new(SAVED).with("Error", Param::Str(e)),
                        }
                    }
                };
                cx.broadcast(message);
            }
            EditorCall::Export { sm } => {
                let (text, ext) = export::simfile(editor.song(), sm);
                let stem = self
                    .target
                    .as_ref()
                    .and_then(|t| t.file.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| export::file_stem(&editor.song().title));
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Simfile", &[ext])
                    .set_file_name(format!("{stem}.{ext}"))
                    .save_file()
                else {
                    return;
                };
                if let Err(e) = std::fs::write(&path, text.as_bytes()) {
                    cx.broadcast(Message::new(SAVED).with("Error", Param::Str(e.to_string())));
                }
            }
            EditorCall::Playtest { from_cursor, auto } => {
                let (song, chart, start) = editor.playtest(from_cursor);
                let mut r = crate::play::Request::song(&self.id, chart, auto);
                r.edited = Some(Arc::new(song));
                r.start = (start > 0.0).then_some(start);
                *self.songs.play.borrow_mut() = Some(r);
            }
            EditorCall::Autosave => {
                let a = editor.autosave(&self.id, crate::play::epoch_ms());
                if let (Some(path), Ok(json)) = (autosave_path(), serde_json::to_string(&a))
                    && let Err(e) = crate::paths::write_whole(&path, json.as_bytes())
                {
                    eprintln!("editor: autosave: {e}");
                }
            }
            EditorCall::Leave => {
                forget_autosave();
                self.listen.borrow_mut().stop();
                if let Some(dir) = self.fresh.borrow_mut().take() {
                    new_song::forget(&self.songs, &self.id, &dir);
                    self.songs.feed.borrow_mut().data = Some(self.songs.data());
                    cx.broadcast(Message::new(FEED));
                }
            }
            EditorCall::Listen { from } | EditorCall::Record { from, .. } => {
                let mut l = self.listen.borrow_mut();
                // This chart's music, which may be another file.
                l.load(editor.chart_index());
                l.poll();
                l.listener
                    .set_cues(ddi_session::listen::cues_for(&call, editor));
                l.ask(from);
            }
            EditorCall::Stop => self.listen.borrow_mut().stop(),
            EditorCall::Cues => self.listen.borrow_mut().listener.set_cues(Cues::of(editor)),
        }
    }

    fn music_time(&self) -> Option<f64> {
        let l = &mut *self.listen.borrow_mut();
        l.audio.tick();
        l.poll();
        l.listener.cue(&mut l.audio);
        l.listener.time(&l.audio)
    }

    fn heard_at(&self, host: f64) -> Option<f64> {
        self.listen
            .borrow()
            .listener
            .heard_at(ddi_platform::HostTime(host))
    }

    fn record_input(&self, layout: &ddi_chart::Layout) -> Option<RecordInput> {
        Some(ddi_session::listen::record_input(
            &crate::store::settings_cached(),
            layout,
            &crate::input::connected_pads(),
        ))
    }

    fn audio(&self) -> Option<Rc<Analysis>> {
        self.listen.borrow_mut().analysis()
    }

    fn audio_coming(&self) -> bool {
        let l = self.listen.borrow();
        l.loading.is_some() || l.making.is_some()
    }

    fn show_audio(&self, picture: Option<&ddi_ui::screens::editor::audio::AudioPicture>) {
        crate::editor_audio::show(picture);
    }

    fn clock(&self) -> Option<f64> {
        Some(self.started.elapsed().as_secs_f64())
    }

    fn skin_field(&self) -> Option<SkinField> {
        Some(ddi_shell::editor_field::skin_field(
            &crate::store::settings_cached(),
        ))
    }

    fn show_field(&self, picture: Option<&FieldPicture>, editor: &Editor) {
        crate::editor_field::show(picture, editor);
    }
}

/// The editor on song `id` at chart `chart`.
pub fn screen(
    env: &Env,
    songs: &Rc<Songs>,
    id: &str,
    chart: usize,
) -> Result<EditorScreen, String> {
    // Unsaved changes from before (a crash, the window closed) come back.
    let recovered = autosaved(id);
    let status = recovered.as_ref().map_or(String::new(), |_| {
        "Unsaved changes recovered from the last time; leave without saving to drop them".into()
    });
    let editor = match recovered {
        Some(a) => Editor::recover(a),
        None => read(id, chart)?,
    };
    Ok(open(env, songs, id, editor, &status, None))
}

/// The editor on song `id` of the library as `editor` has it; `fresh`:
/// a new song's folder, never saved (removed when left unsaved).
pub fn open(
    env: &Env,
    songs: &Rc<Songs>,
    id: &str,
    editor: Editor,
    status: &str,
    fresh: Option<PathBuf>,
) -> EditorScreen {
    let target = target(id);
    let place = EditorPlace {
        save: target.as_ref().map(|t| t.file.display().to_string()),
        confirm: target
            .as_ref()
            .filter(|t| t.overwrites && fresh.is_none())
            .map(|t| {
                let name = t
                    .file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                format!(
                    "{} is replaced; the file as it was is kept beside it as {name}.old (at the \
                 first save each time the editor is opened).",
                    t.file.display()
                )
            }),
        export: Some("Save a copy".into()),
        listen: true,
        playtest: true,
        discard: fresh
            .as_ref()
            .map(|_| "The new song is not kept: its folder is removed.".into()),
    };
    let host = Host {
        songs: songs.clone(),
        id: id.to_string(),
        target,
        // A new song's first file is not worth an `.old`.
        backed_up: Cell::new(fresh.is_some()),
        listen: RefCell::new(Listen::new(id, editor.chart_index())),
        fresh: RefCell::new(fresh),
        started: std::time::Instant::now(),
    };
    EditorScreen::new(env.clone(), editor, place)
        .with_host(Rc::new(host))
        .with_status(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/editor-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saving_keeps_the_old_file_once_and_reads_back() {
        let dir = scratch("save");
        let file = dir.join("song.ssc");
        std::fs::write(&file, "#TITLE:Old;\n#BPMS:0=120;\n").unwrap();
        let mut e = Editor::new_song(Some("song.ogg".into()), "New");
        e.toggle_at(1, ddi_chart::Tick::from_beats(2));
        save(e.song(), &file, true).unwrap();
        let old = std::fs::read_to_string(dir.join("song.ssc.old")).unwrap();
        assert!(old.starts_with("#TITLE:Old;"));
        let back =
            ddi_chart::formats::sm::parse_ssc(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(back.title, "New");
        assert_eq!(back.charts[0].notes, e.song().charts[0].notes);
        // A second save keeps the first `.old`.
        save(e.song(), &file, false).unwrap();
        assert!(
            std::fs::read_to_string(dir.join("song.ssc.old"))
                .unwrap()
                .starts_with("#TITLE:Old;")
        );
        // No temporary file left.
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    /// An `.sm` song of the library edited, saved as `.ssc` beside it
    /// (read before the `.sm` from then on), read back as edited, and
    /// playtested part-way without being kept.
    #[test]
    fn an_sm_song_saved_beside_itself_read_back_and_playtested() {
        use std::cell::RefCell;
        use std::sync::atomic::{AtomicU64, Ordering};

        use ddi_chart::Tick;
        use ddi_platform::HostTime;
        use ddi_platform::history::MemoryPlayStore;
        use ddi_ui::state::history::Plays;
        use ddi_ui::state::settings::Settings;

        use crate::play::{Ended, Feed, Play};

        let root = scratch("library");
        let song = root.join("Pack/Song");
        std::fs::create_dir_all(&song).unwrap();
        std::fs::write(
            song.join("s.sm"),
            "#TITLE:Song;#ARTIST:Me;#MUSIC:song.wav;#OFFSET:0;#BPMS:0=120;\
             #NOTES:dance-single:x:Easy:3:0,0,0,0,0:1000\n0100\n0010\n0001\n;",
        )
        .unwrap();
        std::fs::write(song.join("song.wav"), crate::library::tests::wav(8.0)).unwrap();
        let (songs, report) = crate::library::scan_folders(std::slice::from_ref(&root));
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        crate::library::register_for_tests(&songs);
        let id = songs[0].entry.id.clone();
        let t = target(&id).expect("saved in place");
        assert_eq!(t.file, song.join("s.ssc"));
        assert!(!t.overwrites);
        let mut e = read(&id, 0).unwrap();
        for beat in 4..12 {
            e.toggle_at((beat % 4) as u8, Tick::from_beats(beat));
        }
        save(e.song(), &t.file, t.overwrites).unwrap();
        assert!(song.join("s.sm").exists(), "the .sm left as it was");
        let (songs, _) = crate::library::scan_folders(std::slice::from_ref(&root));
        crate::library::register_for_tests(&songs);
        assert_eq!(songs[0].entry.id, id);
        assert_eq!(songs[0].entry.chart.as_str(), "s.ssc");
        assert!(target(&id).unwrap().overwrites, "now saved over");
        let back = read(&id, 0).unwrap();
        assert_eq!(back.song().charts, e.song().charts);
        assert_eq!(back.song().timing, e.song().timing);

        // A playtest from beat 8 (2 s before it: beat 4).
        e.set_cursor(Tick::from_beats(8), false);
        let (edited, chart, start) = e.playtest(true);
        assert_eq!(start, 2.0);
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let loaded =
            crate::play::load::load_with(&id, Some(chart), 48_000, false, Some(&edited), &cancel)
                .unwrap();
        assert_eq!(loaded.song, edited);
        let mut request = crate::play::Request::song(&id, chart, true);
        request.edited = Some(Arc::new(edited));
        request.start = Some(start);
        let clock = Arc::new(AtomicU64::new(1_000_000_000));
        let c = clock.clone();
        let audio =
            crate::audio::Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
        let store = Rc::new(MemoryPlayStore::default());
        let plays = Plays::new(store.clone());
        let mut p = Play::start(
            request,
            Rc::new(loaded),
            &Settings::default(),
            audio,
            Rc::new(RefCell::new(Feed::default())),
            None,
            60.0,
        )
        .unwrap();
        let results = loop {
            let now = clock.fetch_add(16_666_667, Ordering::SeqCst) + 16_666_667;
            match p.tick(HostTime(now as f64 / 1e9), Some(&plays)) {
                Some(Ended::Finished { results, .. }) => break *results,
                Some(_) => panic!("the playtest did not finish"),
                None => {}
            }
            assert!(now < 1_000_000_000 * 100, "never ends");
        };
        // The notes from beat 4 on: all eight, top tier.
        assert_eq!(p.session.player.tally().taps[0], 8);
        assert!(!results.failed);
        assert_eq!(store.len(), 0, "a playtest is never kept");
    }
}
