//! Announcers (`docs/plans/announcers.md`, steps 2 and 3): ours, which
//! come with the game, listed in `announcers/index.json` (`xtask
//! gen-announcers`) and their sounds fetched from `announcers/<name>/`
//! ([`bundled`]); and the imported, each kept as
//! `ddi_library::announcer::Kept` in the `announcers` store under its key,
//! its lines' sounds in `files` under `announcer/<key>/<path>` (the path
//! relative to the announcer's folder). Only the lines spoken are stored;
//! the names no line uses stay out. Options lists them (ours first),
//! removes an imported one and speaks a line of the one chosen
//! ([`speak_sample`]); a play decodes the sounds it can speak behind its
//! prompt ([`load_voice`], step 4); song select's lines are decoded when
//! first asked for and spoken on its sound ([`announce`], step 5).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use ddi_library::announcer::{Bundled, Index, Kept, Line, is_bundled};
use ddi_library::path::RelPathBuf;
use ddi_platform::AudioBackend;
use ddi_session::announcer::scene::director::StagedVoice;
use ddi_session::announcer::{Bags, MenuVoice, SharedVoice};
use wasm_bindgen::JsValue;
use web_sys::AudioBuffer;
use web_sys::Blob;

use crate::web::audio::WebAudio;
use crate::web::idb::{self, Db, Write};

/// Key of an announcer's sound in the `files` store.
pub(crate) fn file_key(key: &str, path: &str) -> String {
    format!("announcer/{key}/{path}")
}

/// The writes that store `kept` with its sounds (`(path, blob)`),
/// replacing one of the same key.
pub(crate) fn store(kept: &Kept, sounds: Vec<(String, Blob)>) -> Result<Vec<Write>, String> {
    let key = kept.key();
    let mut writes = vec![Write::DeletePrefix {
        store: idb::FILES,
        prefix: file_key(&key, ""),
    }];
    for (path, blob) in sounds {
        writes.push(Write::Put {
            store: idb::FILES,
            key: file_key(&key, &path),
            value: blob.into(),
        });
    }
    let json = serde_json::to_string(kept).map_err(|e| e.to_string())?;
    writes.push(Write::Put {
        store: idb::ANNOUNCERS,
        key,
        value: JsValue::from_str(&json),
    });
    Ok(writes)
}

/// Every imported announcer.
pub(crate) async fn all(db: &Db) -> Result<Vec<Kept>, String> {
    Ok(db
        .entries(idb::ANNOUNCERS)
        .await?
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str(&v.as_string()?).ok())
        .collect())
}

thread_local! {
    /// Ours, once read.
    static BUNDLED: RefCell<Option<Vec<Bundled>>> = const { RefCell::new(None) };
}

/// Ours, from `announcers/index.json` (fetched once a session; a failure
/// is not kept, the next call tries again).
pub(crate) async fn bundled() -> Result<Vec<Bundled>, String> {
    if let Some(b) = BUNDLED.with_borrow(Clone::clone) {
        return Ok(b);
    }
    // Revalidated each session: its hashes name the sounds.
    let text = crate::songs::fetch_text_fresh("announcers/index.json").await?;
    let index: Index =
        serde_json::from_str(&text).map_err(|e| format!("announcers/index.json: {e}"))?;
    BUNDLED.with_borrow_mut(|b| *b = Some(index.announcers.clone()));
    Ok(index.announcers)
}

/// The announcers one can choose `(key, name)`: ours and the imported
/// (Options orders them). An error (storage or ours unread) keeps the
/// choice as it is.
pub(crate) async fn choices() -> Result<Vec<(String, String)>, String> {
    let ours = bundled().await?;
    let db = Db::open().await?;
    Ok(ours
        .into_iter()
        .map(|b| b.kept)
        .chain(all(&db).await?)
        .map(|k| (k.key(), k.name))
        .collect())
}

/// Removes an imported announcer and its sounds (never one of ours).
pub(crate) async fn delete(key: &str) -> Result<(), String> {
    if is_bundled(key) {
        return Err("it comes with the game".into());
    }
    let db = Db::open().await?;
    db.write(vec![
        Write::Delete {
            store: idb::ANNOUNCERS,
            key: key.to_string(),
        },
        Write::DeletePrefix {
            store: idb::FILES,
            prefix: file_key(key, ""),
        },
    ])
    .await
}

/// Announcer `key`'s record.
async fn kept(db: &Db, key: &str) -> Result<Kept, String> {
    let json = db
        .get(idb::ANNOUNCERS, key)
        .await?
        .and_then(|v| v.as_string())
        .ok_or_else(|| format!("no announcer {key}"))?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

/// Where an announcer's sounds are.
enum Source {
    /// The browser's storage: an imported one's.
    Stored(Db),
    /// The site: ours, in `announcers/<path>/`, each sound fetched under
    /// its hash.
    Bundled {
        path: String,
        hashes: BTreeMap<String, String>,
    },
}

/// Announcer `key`'s record, and where its sounds are.
async fn open(key: &str) -> Result<(Kept, Source), String> {
    if is_bundled(key) {
        let b = bundled()
            .await?
            .into_iter()
            .find(|b| b.kept.key() == key)
            .ok_or_else(|| format!("no announcer {key}"))?;
        return Ok((
            b.kept,
            Source::Bundled {
                path: b.path,
                hashes: b.hashes,
            },
        ));
    }
    let db = Db::open().await?;
    let kept = kept(&db, key).await?;
    Ok((kept, Source::Stored(db)))
}

/// The bytes of announcer `key`'s sound `path` (a path of
/// [`Kept::lines`]).
async fn sound(source: &Source, key: &str, path: &str) -> Result<Vec<u8>, String> {
    match source {
        Source::Stored(db) => {
            let blob: Blob = db
                .get(idb::FILES, &file_key(key, path))
                .await?
                .ok_or_else(|| format!("{path} is not stored"))?
                .into();
            crate::web::files::blob_bytes(&blob).await
        }
        Source::Bundled { path: dir, hashes } => {
            let hash = hashes.get(path).map(String::as_str);
            let url = crate::songs::bundled_url("announcers", dir, path, hash);
            crate::songs::fetch_bytes(&url).await
        }
    }
}

/// The bytes of our announcer `pack`'s sound `path` (the announcer
/// studio's examples).
pub(crate) async fn bundled_sound(pack: &Bundled, path: &str) -> Result<Vec<u8>, String> {
    let source = Source::Bundled {
        path: pack.path.clone(),
        hashes: pack.hashes.clone(),
    };
    sound(&source, &pack.kept.key(), path).await
}

thread_local! {
    /// The lines' shuffles, for the session of the page.
    static BAGS: RefCell<Bags> = RefCell::new(Bags::new(
        (js_sys::Math::random() * 9_007_199_254_740_991.0) as u64,
    ));
}

/// Whose sounds: a play's (of a chart of `notes` notes), or song
/// select's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum For {
    Play { notes: u32 },
    Menu,
}

/// How many of an announcer's sounds are read and decoded at once.
const AT_ONCE: usize = 4;

/// Decodes into `voice` the sounds of announcer `key` `of` a play
/// (`ddi_session::announcer::wanted`) or song select
/// (`ddi_session::announcer::menu`, the line waiting spoken once its sound
/// is in), on an offline context (no gesture needed; they play on the
/// play's or song select's, resampled if need be): [`AT_ONCE`] at a
/// time, those said first asked for first; stops once `alive` says the
/// page went.
pub(crate) fn load_voice(
    key: String,
    of: For,
    voice: SharedVoice<AudioBuffer>,
    alive: Rc<Cell<bool>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let started = async {
            let (kept, source) = open(&key).await?;
            let wanted = BAGS.with_borrow_mut(|b| match of {
                For::Play { notes } => ddi_session::announcer::wanted(&kept, notes, b),
                For::Menu => ddi_session::announcer::menu(&kept, b),
            });
            voice
                .borrow_mut()
                .expect(wanted.iter().map(|(line, _)| *line));
            let decoder = crate::web::audio::AudioDecoder::offline(48_000.0)
                .ok_or("this browser cannot decode sounds ahead")?;
            let queue = wanted
                .into_iter()
                .flat_map(|(line, files)| {
                    files
                        .into_iter()
                        .enumerate()
                        .map(move |(rank, f)| (line, rank, f))
                })
                .collect();
            Ok::<_, String>((source, decoder, queue))
        }
        .await;
        match started {
            Ok((source, decoder, queue)) => {
                let load = Rc::new(Load {
                    key,
                    of,
                    voice,
                    alive,
                    source,
                    decoder,
                    queue: RefCell::new(queue),
                });
                for _ in 0..AT_ONCE {
                    wasm_bindgen_futures::spawn_local(load.clone().run());
                }
            }
            Err(e) => web_sys::console::warn_1(&format!("announcer: {e}").into()),
        }
    });
}

/// An announcer's sounds being read and decoded ([`load_voice`]).
struct Load {
    key: String,
    of: For,
    voice: SharedVoice<AudioBuffer>,
    alive: Rc<Cell<bool>>,
    source: Source,
    decoder: crate::web::audio::AudioDecoder,
    /// The sounds not asked for yet, in the order asked for, each with
    /// its rank among its line's: their decodes finish in any order.
    queue: RefCell<VecDeque<(Line, usize, RelPathBuf)>>,
}

impl Load {
    /// Reads and decodes the next sound left, one at a time, until none
    /// is.
    async fn run(self: Rc<Load>) {
        while self.alive.get() {
            let Some((line, rank, file)) = self.queue.borrow_mut().pop_front() else {
                return;
            };
            let path = file.to_string();
            let decoded = match sound(&self.source, &self.key, &path).await {
                Ok(bytes) => self.decoder.decode(&bytes, Default::default()).await,
                Err(e) => Err(e),
            };
            match decoded {
                Ok(buffer) => self.voice.borrow_mut().add_ranked(line, rank, buffer),
                Err(e) => web_sys::console::warn_1(&format!("announcer: {path}: {e}").into()),
            }
            if self.of == For::Menu {
                speak_waiting(&self.key);
            }
        }
    }
}

/// Song select's lines, for the announcer chosen.
struct Menu {
    voice: MenuVoice<AudioBuffer>,
    /// Its load goes on (until another announcer is chosen).
    alive: Rc<Cell<bool>>,
    /// The lines spoken (`__DDI_DEBUG.menuAnnouncer`).
    spoken: Vec<String>,
}

/// Seconds on the clock song select's lines are timed on.
fn menu_now() -> f64 {
    js_sys::Date::now() / 1000.0
}

thread_local! {
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

/// The announcer studio's voice while its scene shows song select
/// (`crate::components::studio_page`): song select's lines are spoken
/// from it in place of the announcer chosen.
type Staged = StagedVoice<AudioBuffer>;

thread_local! {
    static STAGED: RefCell<Option<Staged>> = const { RefCell::new(None) };
}

/// The studio's voice speaks song select's lines from now (`None`: the
/// announcer chosen again).
pub(crate) fn stage(staged: Option<Staged>) {
    STAGED.with_borrow_mut(|s| *s = staged);
}

/// Speaks song select's `line` (`UiRequest::Announce`) where the
/// announcer chosen has it: its sounds decoded the first time (the line
/// then spoken once decoded, if that is soon), on song select's sound
/// (`crate::select_preview::speak`). The studio's voice instead while
/// it stages song select ([`stage`]).
pub(crate) fn announce(line: Line) {
    let staged = STAGED.with_borrow_mut(|s| {
        let s = s.as_mut()?;
        let spoke = s.voice.next(line).and_then(|(found, buffer)| {
            crate::select_preview::speak_staged(buffer, s.share).map(|len| (found, len))
        });
        Some((s.told.clone(), spoke))
    });
    if let Some((told, spoke)) = staged {
        told(line, spoke);
        return;
    }
    let settings = crate::settings::Settings::load();
    let Some(key) = settings.announcer.clone() else {
        return;
    };
    let fresh = MENU.with_borrow(|m| m.as_ref().is_none_or(|m| m.voice.key() != key));
    if fresh {
        let voice = SharedVoice::default();
        let alive = Rc::new(Cell::new(true));
        let old = MENU.with_borrow_mut(|m| {
            m.replace(Menu {
                voice: MenuVoice::new(key.clone(), voice.clone()),
                alive: alive.clone(),
                spoken: Vec::new(),
            })
        });
        if let Some(old) = old {
            old.alive.set(false);
        }
        load_voice(key, For::Menu, voice, alive);
    }
    let found = MENU.with_borrow_mut(|m| m.as_mut()?.voice.ask(line, menu_now()));
    speak_menu(found, &settings);
}

/// Speaks song select's line found, if any.
fn speak_menu(found: Option<(Line, AudioBuffer)>, settings: &crate::settings::Settings) {
    let Some((found, buffer)) = found else {
        return;
    };
    if crate::select_preview::speak(&buffer, settings.announcer_volume, settings.volume) {
        MENU.with_borrow_mut(|m| {
            if let Some(m) = m {
                m.spoken.push(found.folder());
            }
        });
    }
}

/// `__DDI_DEBUG.menuAnnouncer`: song select's lines spoken this session
/// (every page's debug state carries it, `crate::web::screen::debug_into`).
pub(crate) fn debug_into(obj: &js_sys::Object) {
    MENU.with_borrow(|m| {
        if let Some(m) = m {
            let list = js_sys::Array::new();
            for s in &m.spoken {
                list.push(&s.as_str().into());
            }
            let _ = js_sys::Reflect::set(obj, &"menuAnnouncer".into(), &list);
        }
    });
}

/// Speaks song select's line waiting, if announcer `key`'s sound for it
/// is in now and it is still due.
fn speak_waiting(key: &str) {
    let found = MENU.with_borrow_mut(|m| {
        m.as_mut()
            .filter(|m| m.voice.key() == key)?
            .voice
            .poll(menu_now())
    });
    if found.is_some() {
        speak_menu(found, &crate::settings::Settings::load());
    }
}

thread_local! {
    /// What Options' last sample did: the sound spoken, or why not
    /// (`__DDI_DEBUG.options.announcer`).
    static SPOKEN: RefCell<Option<Result<String, String>>> = const { RefCell::new(None) };
}

/// What Options' last sample did.
pub(crate) fn spoken() -> Option<Result<String, String>> {
    SPOKEN.with_borrow(Clone::clone)
}

/// A sample line speaking: its own context, closed when it is dropped.
pub(crate) struct Sample(#[allow(dead_code)] Rc<RefCell<WebAudio>>);

/// Speaks a line of announcer `key` (`Kept::sample`) at `share` of the
/// music volume `volume` on a context made now (call it inside the
/// press: Safari starts one only there); the line is read and decoded
/// after, and dropped unheard if the sample is dropped meanwhile.
pub(crate) fn speak_sample(key: &str, volume: f32, share: f32) -> Result<Sample, String> {
    let audio = WebAudio::new()?;
    audio.resume_now();
    audio.set_master_volume(volume);
    let audio = Rc::new(RefCell::new(audio));
    let weak = Rc::downgrade(&audio);
    let key = key.to_string();
    let decoder = audio.borrow().decoder();
    wasm_bindgen_futures::spawn_local(async move {
        let spoke = async {
            let (kept, source) = open(&key).await?;
            let pick = (js_sys::Math::random() * 1e9) as usize;
            let path = kept
                .sample(pick)
                .ok_or("the announcer has no sounds")?
                .to_string();
            let bytes = sound(&source, &key, &path).await?;
            decoder.resume().await;
            let buffer = decoder.decode(&bytes, Default::default()).await?;
            let Some(audio) = weak.upgrade() else {
                return Err("left before it was decoded".to_string());
            };
            let mut audio = audio.borrow_mut();
            let at = audio.context().current_time() + 0.05;
            audio.play_voice(&buffer, at, share)?;
            Ok(path)
        }
        .await;
        if let Err(e) = &spoke {
            web_sys::console::warn_1(&format!("announcer sample: {e}").into());
        }
        SPOKEN.with_borrow_mut(|s| *s = Some(spoke));
    });
    Ok(Sample(audio))
}
