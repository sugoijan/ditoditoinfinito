//! A new song from a music file on desktop (`docs/plans/editor.md`, "New
//! songs"): a folder of its own in the pack folder [`PACK`] under the
//! first song folder Options lists (its `Songs` for a StepMania root;
//! with none listed, `Songs` in the data folder, listed from then on),
//! the music copied in and `<folder>.ssc` with one empty chart written,
//! read into the library at once; the editor opens on it unsaved. Left
//! before its first save, the folder is removed again ([`forget`]).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ddi_library::pack::song_id;
use ddi_library::tags;
use ddi_ui::screens::Env;
use ddi_ui::screens::editor::EditorScreen;
use ddi_ui::state::editor::Editor;
use ddi_ui::state::editor::new_song::{NewSong, PACK};

use crate::songs::Songs;

/// A new song made: its id, the editor on it, its folder.
pub struct Made {
    pub id: String,
    pub editor: Editor,
    pub dir: PathBuf,
}

/// The folder new songs' pack folder goes in.
fn base(songs: &Songs) -> Result<PathBuf, String> {
    if let Some(f) = songs.folders().into_iter().find(|f| f.is_dir()) {
        return Ok(crate::library::songs_dir(&f));
    }
    let dir = crate::paths::data_dir()
        .ok_or("no data folder")?
        .join("Songs");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    songs.add_folders(vec![dir.clone()]);
    Ok(dir)
}

/// Makes the song for the music at `music` and puts it in the library.
pub fn create(songs: &Songs, music: &Path) -> Result<Made, String> {
    let pack = base(songs)?.join(PACK);
    std::fs::create_dir_all(&pack).map_err(|e| format!("{}: {e}", pack.display()))?;
    let name = music
        .file_name()
        .ok_or("no file name")?
        .to_string_lossy()
        .into_owned();
    let mut head = Vec::new();
    std::fs::File::open(music)
        .and_then(|f| f.take(tags::HEAD as u64).read_to_end(&mut head))
        .map_err(|e| format!("{}: {e}", music.display()))?;
    let plan = NewSong::plan(&name, &head, |f| {
        // A `Songs` in the pack folder would read as a StepMania root.
        f.eq_ignore_ascii_case("Songs")
            || pack.join(f).exists()
            || crate::library::song(&song_id(PACK, f)).is_some()
    });
    let dir = pack.join(&plan.folder);
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let made = (|| {
        std::fs::copy(music, dir.join(&plan.music))
            .map_err(|e| format!("copying the music: {e}"))?;
        let editor = plan.editor();
        super::save(
            editor.song(),
            &dir.join(format!("{}.ssc", plan.folder)),
            false,
        )?;
        let id = plan.id();
        let (found, _) = crate::library::scan_folders(std::slice::from_ref(&pack));
        let song = found
            .into_iter()
            .find(|s| s.entry.id == id)
            .ok_or("the new song could not be read")?;
        songs.put_songs(vec![song], &[]);
        Ok(Made {
            id,
            editor,
            dir: dir.clone(),
        })
    })();
    if made.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    made
}

/// New song `id`, never saved, taken away: its folder `dir` removed, the
/// library told.
pub fn forget(songs: &Songs, id: &str, dir: &Path) {
    if let Err(e) = std::fs::remove_dir_all(dir) {
        eprintln!("editor: removing {}: {e}", dir.display());
    }
    songs.put_songs(Vec::new(), &[id.to_string()]);
}

/// Song select's "New song": the music picked (`crate::os::pick_music`),
/// the song made, the editor on it; `None` when the picker was cancelled.
pub fn pick(env: &Env, songs: &Rc<Songs>) -> Option<Result<EditorScreen, String>> {
    let music = crate::os::pick_music()?;
    Some(create(songs, &music).map(|m| {
        super::open(
            env,
            songs,
            &m.id,
            m.editor,
            ddi_ui::state::editor::new_song::STATUS,
            Some(m.dir),
        )
    }))
}
