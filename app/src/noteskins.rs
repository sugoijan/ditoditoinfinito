//! Imported noteskins: their records in IndexedDB, and readying the chosen
//! one for a play (its images decoded, its actors built for the layout).
//! The skin logic is `ddi_noteskin`'s; drawing is `ddi_render::noteskin`'s.

use std::cell::RefCell;
use std::collections::HashMap;

use ddi_chart::Layout;
use ddi_noteskin::build::{Images, TextureKey};
use ddi_noteskin::{Dir, Folder, Skin};
use ddi_render::noteskin::NoteskinPlay;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, ImageBitmap};

use crate::web::idb::{self, Db, Write};

/// An imported skin folder, as stored.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredSkin {
    pub(crate) game: String,
    pub(crate) name: String,
    /// Every file, relative to the skin folder.
    pub(crate) files: Vec<String>,
    /// `metrics.ini` and `.redir` texts.
    pub(crate) texts: Vec<(String, String)>,
    /// Where it was imported from (the folder or zip path).
    #[serde(default)]
    pub(crate) origin: String,
    /// Files kept in storage (images, Lua, models), by path.
    #[serde(default)]
    pub(crate) stored: Vec<String>,
}

impl StoredSkin {
    /// The store key, `<game>/<name>` (names match ignoring case, as
    /// StepMania's do).
    pub(crate) fn key(&self) -> String {
        skin_key(&self.game, &self.name)
    }

    fn folder(&self) -> Folder {
        Folder {
            game: self.game.clone(),
            name: self.name.clone(),
            files: self.files.clone(),
            texts: self.texts.clone(),
        }
    }
}

pub(crate) fn skin_key(game: &str, name: &str) -> String {
    format!(
        "{}/{}",
        game.to_ascii_lowercase(),
        name.to_ascii_lowercase()
    )
}

/// Key of a skin's file in the `files` store.
pub(crate) fn file_key(skin: &str, path: &str) -> String {
    format!("noteskin/{skin}/{path}")
}

/// Images and scripts a skin keeps, at most; larger skins are cut and
/// reported (its `metrics.ini` and redirects are always kept).
pub(crate) const MAX_FILES: usize = 2000;
/// Skins one import keeps, at most.
pub(crate) const MAX_SKINS: usize = 200;
/// The largest `metrics.ini` or `.redir` read, in bytes.
pub(crate) const MAX_TEXT: u64 = 1 << 20;

/// Whether a skin's file is worth storing: images, Lua and models (for the
/// steps that read them), the texts.
pub(crate) fn kept_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".lua", ".txt", ".ini", ".redir",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
}

/// Every imported skin.
pub(crate) async fn all(db: &Db) -> Result<Vec<StoredSkin>, String> {
    Ok(db
        .entries(idb::NOTESKINS)
        .await?
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str(&v.as_string()?).ok())
        .collect())
}

/// The writes that store `skin` (replacing one of the same name).
pub(crate) fn store(skin: &StoredSkin, files: Vec<(String, Blob)>) -> Result<Vec<Write>, String> {
    let key = skin.key();
    let mut writes = vec![Write::DeletePrefix {
        store: idb::FILES,
        prefix: file_key(&key, ""),
    }];
    for (path, blob) in files {
        writes.push(Write::Put {
            store: idb::FILES,
            key: file_key(&key, &path),
            value: blob.into(),
        });
    }
    let json = serde_json::to_string(skin).map_err(|e| e.to_string())?;
    writes.push(Write::Put {
        store: idb::NOTESKINS,
        key,
        value: JsValue::from_str(&json),
    });
    Ok(writes)
}

/// Removes an imported skin and its files.
pub(crate) async fn delete(key: &str) -> Result<(), String> {
    let db = Db::open().await?;
    db.write(vec![
        Write::Delete {
            store: idb::NOTESKINS,
            key: key.to_string(),
        },
        Write::DeletePrefix {
            store: idb::FILES,
            prefix: file_key(key, ""),
        },
    ])
    .await
}

/// The dance skins one can pick: `(key, name)`.
pub(crate) async fn choices() -> Result<Vec<(String, String)>, String> {
    let db = Db::open().await?;
    let mut skins: Vec<(String, String)> = all(&db)
        .await?
        .into_iter()
        .filter(|s| s.game == "dance" && !s.name.eq_ignore_ascii_case("default"))
        .map(|s| (s.key(), s.name))
        .collect();
    skins.sort_by_key(|(_, n)| n.to_lowercase());
    Ok(skins)
}

/// What each element of a skin is, in a line for the import report: how
/// many of a column's elements are the skin's images, and why the rest are
/// the built-in skin's.
pub(crate) fn summary(folders: &[Folder], game: &str, name: &str) -> String {
    use ddi_noteskin::load::ENGINE_ELEMENTS;
    use ddi_noteskin::{Resolved, Source};
    let skin = match Skin::load(folders, game, name) {
        Ok(s) => s,
        Err(e) => return e.to_string(),
    };
    let (mut own, mut lua, mut model, mut other, mut builtin) = (0, 0, 0, 0, 0);
    for e in ENGINE_ELEMENTS {
        match skin.resolve(ddi_noteskin::Button::Down, e) {
            Resolved::File {
                source: Source::Image(_),
                ..
            }
            | Resolved::Blank => own += 1,
            Resolved::File {
                source: Source::Lua(_),
                ..
            } => lua += 1,
            Resolved::File {
                source: Source::Model(_),
                ..
            } => model += 1,
            Resolved::File {
                source: Source::BuiltInArt(_) | Source::Native(..),
                ..
            }
            | Resolved::Missing(_) => builtin += 1,
            _ => other += 1,
        }
    }
    let mut parts = vec![format!(
        "{own} of {} elements are its own images",
        ENGINE_ELEMENTS.len()
    )];
    if lua > 0 {
        parts.push(format!("{lua} are Lua (not run yet: drawn built in)"));
    }
    if model > 0 {
        parts.push(format!("{model} are 3D models (drawn built in)"));
    }
    if builtin > 0 {
        parts.push(format!(
            "{builtin} come from StepMania's own skins (drawn built in)"
        ));
    }
    if other > 0 {
        parts.push(format!("{other} cannot be drawn"));
    }
    let scripts = skin.ignored_scripts();
    if !scripts.is_empty() {
        parts.push(format!(
            "its NoteSkin.lua does not run yet ({})",
            scripts.join(", ")
        ));
    }
    parts.join("; ")
}

/// A skin ready for a play: its actors and the decoded images its
/// textures stand for (`None` for the built-in art).
pub(crate) struct Prepared {
    pub(crate) play: NoteskinPlay,
    pub(crate) images: Vec<Option<ImageBitmap>>,
}

impl Prepared {
    /// Frees the decoded images (a skin readied but never shown).
    pub(crate) fn close(self) {
        for b in self.images.into_iter().flatten() {
            b.close();
        }
    }
}

/// Image sizes as decoded (original pixels), by `(folder, path)`.
struct Sizes {
    sizes: HashMap<(usize, String), (u32, u32)>,
    max_size: u32,
    /// The images asked for (the first pass answers every size 128).
    asked: RefCell<Vec<(usize, String)>>,
    probe: bool,
}

impl Images for Sizes {
    fn size(&self, dir: Dir, path: &str) -> Option<(u32, u32)> {
        let Dir::Folder(i) = dir else {
            return None;
        };
        if self.probe {
            self.asked.borrow_mut().push((i, path.to_string()));
            return Some((128, 128));
        }
        self.sizes.get(&(i, path.to_string())).copied()
    }

    fn max_size(&self) -> u32 {
        self.max_size
    }
}

/// The largest image side kept (GPU limits and memory).
const MAX_IMAGE_SIDE: u32 = 4096;

/// Decoded pixels a skin may hold at once (each is kept as a bitmap and
/// again on the GPU): about 256 MB of each. Images past it are left out
/// and drawn by the built-in skin.
const MAX_PIXELS: u64 = 64 << 20;

/// Readies skin `key` for `layout`: `None` when there is no such skin, it
/// does not load, or the layout's lanes are not dance buttons.
pub(crate) async fn prepare(key: &str, layout: &Layout) -> Option<Prepared> {
    let db = Db::open().await.ok()?;
    let stored = all(&db).await.ok()?;
    let chosen = stored.iter().find(|s| s.key() == key)?;
    let folders: Vec<Folder> = stored.iter().map(StoredSkin::folder).collect();
    let skin = match Skin::load(&folders, &chosen.game, &chosen.name) {
        Ok(s) => s,
        Err(e) => {
            web_sys::console::warn_1(&format!("noteskin {}: {e}", chosen.name).into());
            return None;
        }
    };
    // First pass: which images the actors use.
    let mut sizes = Sizes {
        sizes: HashMap::new(),
        max_size: MAX_IMAGE_SIDE,
        asked: RefCell::new(Vec::new()),
        probe: true,
    };
    NoteskinPlay::new(&skin, layout, &sizes, true)?;
    let mut asked = sizes.asked.take();
    asked.sort();
    asked.dedup();
    let mut bitmaps: HashMap<(usize, String), ImageBitmap> = HashMap::new();
    let mut pixels: u64 = 0;
    for (i, path) in asked {
        if pixels >= MAX_PIXELS {
            web_sys::console::warn_1(
                &format!("noteskin {}: {path} left out: the skin's images are too large together", stored[i].name).into(),
            );
            continue;
        }
        let skin_key = stored[i].key();
        let blob = db
            .get(idb::FILES, &file_key(&skin_key, &path))
            .await
            .ok()
            .flatten()
            .and_then(|v| v.dyn_into::<Blob>().ok());
        let Some(blob) = blob else {
            web_sys::console::warn_1(
                &format!("noteskin {}: {path} is not stored", stored[i].name).into(),
            );
            continue;
        };
        match crate::web::image::decode_sized(&blob, MAX_IMAGE_SIDE).await {
            Ok((bitmap, size)) => {
                pixels += u64::from(bitmap.width()) * u64::from(bitmap.height());
                sizes.sizes.insert((i, path.clone()), size);
                bitmaps.insert((i, path), bitmap);
            }
            Err(e) => web_sys::console::warn_1(
                &format!("noteskin {}: {path}: {e}", stored[i].name).into(),
            ),
        }
    }
    sizes.probe = false;
    let Some(play) = NoteskinPlay::new(&skin, layout, &sizes, true) else {
        for b in bitmaps.values() {
            b.close();
        }
        return None;
    };
    let images = play
        .texture_keys()
        .iter()
        .map(|k| match k {
            TextureKey::Skin {
                dir: Dir::Folder(i),
                path,
            } => bitmaps.get(&(*i, path.clone())).cloned(),
            _ => None,
        })
        .collect();
    for (_, b) in bitmaps.iter().filter(|(k, _)| {
        !play.texture_keys().iter().any(|t| matches!(t, TextureKey::Skin { dir: Dir::Folder(i), path } if (*i, path) == (k.0, &k.1)))
    }) {
        b.close();
    }
    for n in &play.notes {
        web_sys::console::info_1(&format!("noteskin {}: {n}", chosen.name).into());
    }
    Some(Prepared { play, images })
}
