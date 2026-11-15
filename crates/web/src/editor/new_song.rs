//! A new song from a music file on the web (`docs/plans/editor.md`, "New
//! songs"): stored at once as an imported song of the pack
//! [`new_song::PACK`] (its music the file picked, its chart `chart.ssc`
//! with one empty chart), so listening and playtests read it as any
//! stored song, and the editor opened on it unsaved. Leaving the editor
//! before a first save takes it away again (`super::forget_new`).

use std::collections::HashSet;

use ddi_chart::formats::sm_write;
use ddi_library::path::RelPathBuf;
use ddi_library::tags;
use ddi_ui::state::editor::new_song::{self, Failure, NewSong};
use wasm_bindgen::JsValue;

use crate::songs::{ImportedSong, file_key};
use crate::web::idb::{self, Db, Write};

/// The chart's file in the stored song (where saving writes it).
const CHART: &str = "chart.ssc";

/// Whether music `file` decodes, as a play decodes it (the browser's
/// decoder, else the built-in ones: `AudioDecoder`), before a song is
/// made of it: a file that does not is refused with the decoder's words
/// and nothing is stored. Where the browser has no offline context to
/// decode on before a gesture, the file is taken as it is, as before.
async fn check_music(file: &web_sys::File) -> Result<(), Failure> {
    let Some(decoder) = crate::web::audio::AudioDecoder::offline(48_000.0) else {
        return Ok(());
    };
    let bytes = crate::web::files::blob_bytes(file).await?;
    decoder.decodes(&bytes).await.map_err(Failure::Music)
}

/// Makes the song for music `file`, stored; the editor's session begins
/// on it. Its id. Music that cannot be decoded makes none
/// ([`check_music`]).
pub(crate) async fn create(file: web_sys::File) -> Result<String, Failure> {
    check_music(&file).await?;
    Ok(make(file).await?)
}

async fn make(file: web_sys::File) -> Result<String, String> {
    let head = file
        .slice_with_f64_and_f64(0.0, tags::HEAD as f64)
        .map_err(|_| "reading the music file".to_string())?;
    let head = crate::web::files::blob_bytes(&head).await?;
    let db = Db::open().await?;
    let ids: HashSet<String> = db
        .keys_with_prefix(idb::SONGS, "u-")
        .await?
        .into_iter()
        .collect();
    let plan = NewSong::plan(&file.name(), &head, |folder| {
        ids.contains(&ddi_library::pack::song_id(new_song::PACK, folder))
    });
    let id = plan.id();
    let editor = plan.editor();
    let text = sm_write::write_ssc(editor.song());
    let entry = ddi_library::edited::entry_for_new(
        &id,
        &text,
        RelPathBuf::from(CHART),
        RelPathBuf::from(plan.music.as_str()),
    )?;
    let chart =
        web_sys::Blob::new_with_str_sequence(&js_sys::Array::of1(&JsValue::from_str(&text)))
            .map_err(|_| "no file".to_string())?;
    let record = ImportedSong {
        entry,
        pack: new_song::PACK.into(),
        dir: format!("{}/{}", new_song::PACK, plan.folder),
        imported: js_sys::Date::now(),
        bytes: file.size() + text.len() as f64,
        folder: None,
        links: Default::default(),
        work: None,
        still: None,
    };
    let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    db.write(vec![
        Write::Put {
            store: idb::FILES,
            key: file_key(&id, CHART),
            value: chart.into(),
        },
        Write::Put {
            store: idb::FILES,
            key: file_key(&id, &plan.music),
            value: JsValue::from(file),
        },
        Write::Put {
            store: idb::SONGS,
            key: id.clone(),
            value: JsValue::from_str(&json),
        },
    ])
    .await?;
    super::begin_new(&id, editor);
    Ok(id)
}
