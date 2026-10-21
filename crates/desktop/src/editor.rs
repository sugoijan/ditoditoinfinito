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
//! is left as it is). Written to a temporary file first and renamed
//! over, so a failed write leaves the old file whole. A bundled song or
//! one read from a zip is not saved in place: a copy only. The song's
//! pack is read again after a save.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use ddi_chart::Song;
use ddi_chart::formats::sm_write;
use ddi_ui::Cx;
use ddi_ui::message::{Message, Param};
use ddi_ui::screens::Env;
use ddi_ui::screens::editor::{EditorCall, EditorHost, EditorPlace, EditorScreen, SAVED};
use ddi_ui::screens::select::{FEED, SelectCall};
use ddi_ui::state::editor::Editor;

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
    let ext = chart.extension()?.to_ascii_lowercase();
    if !matches!(ext.as_str(), "ssc" | "sm" | "dwi") {
        return None;
    }
    let file = if ext == "ssc" {
        s.dir.join(chart.as_str())
    } else {
        s.dir
            .join(chart.parent().as_str())
            .join(format!("{}.ssc", chart.file_stem()))
    };
    let file = file.to_path(&s.root)?;
    Some(Target {
        overwrites: ext == "ssc" || file.exists(),
        file,
        pack: s.pack.clone(),
    })
}

/// Song `id` as the editor opens it: its simfile read, at chart `chart`.
pub fn read(id: &str, chart: usize) -> Result<Editor, String> {
    let (files, entry) = crate::play::load::Files::of(id).ok_or_else(|| format!("no song {id}"))?;
    if crate::library::song(id).is_some_and(|s| s.work.is_some())
        || ddi_library::ffr::is_level_chart(&entry.chart)
    {
        return Err("Only StepMania and DWI songs can be edited.".into());
    }
    let bytes = files.read(&entry.chart)?;
    let text = ddi_library::pack::decode_text_with(&bytes, crate::library::legacy_text);
    let ext = entry.chart.extension().unwrap_or("sm").to_ascii_lowercase();
    let song = ddi_library::scan::parse_simfile(&text, &ext).map_err(|e| e.to_string())?;
    Ok(Editor::open(song, chart))
}

/// Writes `song` as `.ssc` to `file`: through a temporary file renamed
/// over it; the file there before kept as `<name>.old` when `backup`.
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
    let tmp = file.with_file_name(format!(".{name}.tmp"));
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

/// The editor's calls on desktop.
struct Host {
    songs: Rc<Songs>,
    id: String,
    target: Option<Target>,
    /// The `.old` was made this session.
    backed_up: Cell<bool>,
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
                let ext = if sm { "sm" } else { "ssc" };
                let stem = self
                    .target
                    .as_ref()
                    .and_then(|t| t.file.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| file_stem_of(&editor.song().title));
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Simfile", &[ext])
                    .set_file_name(format!("{stem}.{ext}"))
                    .save_file()
                else {
                    return;
                };
                let text = if sm {
                    sm_write::write_sm(editor.song())
                } else {
                    sm_write::write_ssc(editor.song())
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
            // Listening waits for its step (`docs/plans/editor.md`, 5).
            EditorCall::Listen { .. } | EditorCall::Stop | EditorCall::Leave => {}
        }
    }
}

/// A file name from a title: what file systems take.
fn file_stem_of(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "song".into() } else { s }
}

/// The editor on song `id` at chart `chart`.
pub fn screen(
    env: &Env,
    songs: &Rc<Songs>,
    id: &str,
    chart: usize,
) -> Result<EditorScreen, String> {
    let editor = read(id, chart)?;
    let target = target(id);
    let place = EditorPlace {
        save: target.as_ref().map(|t| t.file.display().to_string()),
        overwrites: target.as_ref().is_some_and(|t| t.overwrites),
        export: Some("Save a copy".into()),
        listen: false,
        playtest: true,
    };
    let host = Host {
        songs: songs.clone(),
        id: id.to_string(),
        target,
        backed_up: Cell::new(false),
    };
    Ok(EditorScreen::new(env.clone(), editor, place).with_host(Rc::new(host)))
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

    /// A silent mono WAV `seconds` long.
    fn wav(seconds: f64) -> Vec<u8> {
        let frames = (seconds * 48_000.0) as u32;
        let mut w = b"RIFF".to_vec();
        w.extend((36 + frames * 2).to_le_bytes());
        w.extend(b"WAVEfmt ");
        w.extend(16u32.to_le_bytes());
        w.extend(1u16.to_le_bytes());
        w.extend(1u16.to_le_bytes());
        w.extend(48_000u32.to_le_bytes());
        w.extend(96_000u32.to_le_bytes());
        w.extend(2u16.to_le_bytes());
        w.extend(16u16.to_le_bytes());
        w.extend(b"data");
        w.extend((frames * 2).to_le_bytes());
        w.resize(w.len() + frames as usize * 2, 0);
        w
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
        std::fs::write(song.join("song.wav"), wav(8.0)).unwrap();
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

    #[test]
    fn file_names_from_titles() {
        assert_eq!(file_stem_of("A/B: c?"), "A_B_ c_");
        assert_eq!(file_stem_of(" .. "), "song");
    }
}
