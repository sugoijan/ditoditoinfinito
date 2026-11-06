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

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ddi_library::announcer::{Bundled, Index, Kept, Line, is_bundled};
use ddi_platform::AudioBackend;
use ddi_session::announcer::{Bags, SharedVoice, Voice};
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

/// The announcers one can choose `(key, name)`: ours, then the imported
/// by name. An error (storage or ours unread) keeps the choice as it is.
pub(crate) async fn choices() -> Result<Vec<(String, String)>, String> {
    let ours = bundled().await?;
    let db = Db::open().await?;
    let mut list: Vec<(String, String)> = all(&db)
        .await?
        .into_iter()
        .map(|k| (k.key(), k.name))
        .collect();
    list.sort_by_key(|(_, name)| name.to_lowercase());
    Ok(ours
        .into_iter()
        .map(|b| (b.kept.key(), b.kept.name))
        .chain(list)
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
            let encoded: Vec<String> = format!("{dir}/{path}")
                .split('/')
                .map(|p| String::from(js_sys::encode_uri_component(p)))
                .collect();
            let mut url = format!("announcers/{}", encoded.join("/"));
            if let Some(hash) = hashes.get(path) {
                url.push_str(&format!("?v={hash}"));
            }
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

/// Decodes into `voice`, one after another, the sounds of announcer
/// `key` a play of a chart of `notes` notes can speak
/// (`ddi_session::announcer::wanted`), on an offline context (no gesture
/// needed; they play on the play's, resampled if need be); stops once
/// `alive` says the page went.
pub(crate) fn load_voice(
    key: String,
    notes: u32,
    voice: SharedVoice<AudioBuffer>,
    alive: Rc<std::cell::Cell<bool>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let loaded = async {
            let (kept, source) = open(&key).await?;
            let wanted = BAGS.with_borrow_mut(|b| ddi_session::announcer::wanted(&kept, notes, b));
            voice
                .borrow_mut()
                .expect(wanted.iter().map(|(line, _)| *line));
            let decoder = crate::web::audio::AudioDecoder::offline(48_000.0)
                .ok_or("this browser cannot decode sounds ahead")?;
            for (line, files) in wanted {
                for file in files {
                    if !alive.get() {
                        return Ok(());
                    }
                    let path = file.to_string();
                    let decoded = match sound(&source, &key, &path).await {
                        Ok(bytes) => decoder.decode(&bytes, Default::default()).await,
                        Err(e) => Err(e),
                    };
                    match decoded {
                        Ok(buffer) => voice.borrow_mut().add(line, buffer),
                        Err(e) => {
                            web_sys::console::warn_1(&format!("announcer: {path}: {e}").into())
                        }
                    }
                }
            }
            Ok::<(), String>(())
        }
        .await;
        if let Err(e) = loaded {
            web_sys::console::warn_1(&format!("announcer: {e}").into());
        }
    });
}

/// How long a menu line asked for while its sound decodes may wait to be
/// spoken, milliseconds (later, its moment has passed).
const MENU_WAIT_MS: f64 = 1500.0;

/// Song select's lines, for the announcer chosen.
struct Menu {
    key: String,
    voice: SharedVoice<AudioBuffer>,
    /// A line asked for before its sound was decoded, and when.
    pending: Option<(Line, f64)>,
    /// The lines spoken (`__DDI_DEBUG.menuAnnouncer`).
    spoken: Vec<String>,
}

thread_local! {
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

/// What a staged line did: the line asked for, and what spoke for it
/// (the line of its chain found) and for how long; `None`: nothing.
pub(crate) type Told = Rc<dyn Fn(Line, Option<(Line, f64)>)>;

/// The announcer studio's voice while its scene shows song select
/// (`crate::components::studio_page`): song select's lines are spoken
/// from it in place of the announcer chosen, at `share` of the music's
/// volume, and each told to the studio.
pub(crate) struct Staged {
    pub(crate) voice: Voice<AudioBuffer>,
    pub(crate) share: f32,
    pub(crate) told: Told,
}

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
    let fresh = MENU.with_borrow(|m| m.as_ref().is_none_or(|m| m.key != key));
    if fresh {
        let voice = SharedVoice::default();
        MENU.with_borrow_mut(|m| {
            *m = Some(Menu {
                key: key.clone(),
                voice: voice.clone(),
                pending: None,
                spoken: Vec::new(),
            })
        });
        load_menu(key, voice);
    }
    if !speak_menu(line, &settings) {
        MENU.with_borrow_mut(|m| {
            if let Some(m) = m {
                m.pending = Some((line, js_sys::Date::now()));
            }
        });
    }
}

/// Speaks `line` from the menu's voice now; whether its sound was there
/// (spoken or not: no context running).
fn speak_menu(line: Line, settings: &crate::settings::Settings) -> bool {
    let sound = MENU.with_borrow(|m| {
        let m = m.as_ref()?;
        let mut voice = m.voice.borrow_mut();
        voice.next(line).map(|(l, b)| (l, b.clone()))
    });
    let Some((found, buffer)) = sound else {
        return false;
    };
    if crate::select_preview::speak(&buffer, settings.announcer_volume, settings.volume) {
        MENU.with_borrow_mut(|m| {
            if let Some(m) = m {
                m.spoken.push(found.folder());
            }
        });
    }
    true
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

/// Decodes song select's sounds of announcer `key` into `voice`
/// (`ddi_session::announcer::menu`), speaking the line waiting for one.
fn load_menu(key: String, voice: SharedVoice<AudioBuffer>) {
    wasm_bindgen_futures::spawn_local(async move {
        let loaded = async {
            let (kept, source) = open(&key).await?;
            let wanted = BAGS.with_borrow_mut(|b| ddi_session::announcer::menu(&kept, b));
            voice
                .borrow_mut()
                .expect(wanted.iter().map(|(line, _)| *line));
            let decoder = crate::web::audio::AudioDecoder::offline(48_000.0)
                .ok_or("this browser cannot decode sounds ahead")?;
            for (line, files) in wanted {
                for file in files {
                    let path = file.to_string();
                    let decoded = match sound(&source, &key, &path).await {
                        Ok(bytes) => decoder.decode(&bytes, Default::default()).await,
                        Err(e) => Err(e),
                    };
                    match decoded {
                        Ok(buffer) => voice.borrow_mut().add(line, buffer),
                        Err(e) => {
                            web_sys::console::warn_1(&format!("announcer: {path}: {e}").into())
                        }
                    }
                    // The line waiting, if it is this one and still due.
                    let waiting = MENU.with_borrow_mut(|m| {
                        let m = m.as_mut().filter(|m| m.key == key)?;
                        let (l, at) = m.pending?;
                        let due = js_sys::Date::now() - at <= MENU_WAIT_MS;
                        let ours = l.chain().contains(&line);
                        if !due || ours {
                            m.pending = None;
                        }
                        (due && ours).then_some(l)
                    });
                    if let Some(l) = waiting {
                        speak_menu(l, &crate::settings::Settings::load());
                    }
                }
            }
            Ok::<(), String>(())
        }
        .await;
        if let Err(e) = loaded {
            web_sys::console::warn_1(&format!("announcer: {e}").into());
        }
    });
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
