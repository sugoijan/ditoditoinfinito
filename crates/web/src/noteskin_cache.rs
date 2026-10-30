//! What two loads of one skin share past its files (`noteskins::FILES`):
//! the SVG workers' drawings (`ddi_noteskin::recent`, as desktop keeps
//! its own) and the component files' parts, and a wait so that a second
//! load of a skin finds them, the first having ended
//! (`docs/plans/shell-sharing.md`, item 1). The start prompt's diagram
//! and the play ready one skin twice, for two layouts and two Lua
//! environments, from the same documents drawn at the same sizes.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use ddi_noteskin::component::PartDocuments;
use ddi_noteskin::recent::{ImageKey, RECENT_SECONDS, RecentImages};
use gloo::timers::callback::Timeout;
use js_sys::{Object, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::ImageBitmap;

/// How long a load waits for another of the same skin at most, ms.
const LOAD_WAIT_MS: u32 = 15_000;

/// The most component files' parts kept.
const MAX_PARTS: usize = 64;

/// A drawing kept: its bitmap (its own copy, the loads' bitmaps closed
/// after their upload) and the worker's hash of its pixels.
struct Kept {
    bitmap: ImageBitmap,
    hash: Option<u64>,
}

thread_local! {
    static DRAWN: RefCell<RecentImages<Kept>> = RefCell::default();
    /// The trim waiting, if any.
    static TRIM: RefCell<Option<Timeout>> = const { RefCell::new(None) };
    /// Component files' parts, by path and text hash.
    static PARTS: RefCell<HashMap<(String, String), PartDocuments>> = RefCell::default();
    /// The skins being loaded: how many loads of each, and the waiters'
    /// `resolve` functions.
    static LOADING: RefCell<HashMap<String, (u32, Vec<js_sys::Function>)>> = RefCell::default();
    /// Drawings taken from the cache so far (`__DDI_SVG_LOADS`'s
    /// `reused`, per load: read and reset by [`take_reused`]).
    static REUSED: Cell<u32> = const { Cell::new(0) };
}

fn now() -> f64 {
    js_sys::Date::now() / 1000.0
}

fn close(images: Vec<Kept>) {
    for k in images {
        k.bitmap.close();
    }
}

/// A copy of drawing `key` of skin `owner` kept by an earlier load: a
/// promise of `{bitmap, hash}` as the SVG worker answers
/// (`web::svg::drawn`), or `None`.
pub(crate) fn reuse(owner: &str, key: &ImageKey) -> Option<Promise> {
    let copy = DRAWN.with_borrow_mut(|d| {
        let kept = d.get(owner, key, now())?;
        let window = web_sys::window()?;
        let copy = window
            .create_image_bitmap_with_image_bitmap(&kept.bitmap)
            .ok()?;
        Some((copy, kept.hash))
    })?;
    REUSED.with(|r| r.set(r.get() + 1));
    let (copy, hash) = copy;
    Some(wasm_bindgen_futures::future_to_promise(async move {
        let bitmap = wasm_bindgen_futures::JsFuture::from(copy).await?;
        let drawn = Object::new();
        Reflect::set(&drawn, &"bitmap".into(), &bitmap)?;
        if let Some(h) = hash {
            Reflect::set(&drawn, &"hash".into(), &format!("{h:x}").into())?;
        }
        Ok(drawn.into())
    }))
}

/// Keeps a copy of `bitmap`, drawing `key` of skin `owner` whose pixels
/// hash to `hash`, for the next load of the skin: copied now (the load
/// closes its own after the upload), kept once the copy is made, the
/// load going on meanwhile.
pub(crate) fn keep(owner: &str, key: ImageKey, bitmap: &ImageBitmap, hash: Option<u64>) {
    let Some(copy) =
        web_sys::window().and_then(|w| w.create_image_bitmap_with_image_bitmap(bitmap).ok())
    else {
        return;
    };
    let owner = owner.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(copy) = wasm_bindgen_futures::JsFuture::from(copy).await else {
            return;
        };
        let Ok(copy) = copy.dyn_into::<ImageBitmap>() else {
            return;
        };
        let pixels = u64::from(copy.width()) * u64::from(copy.height());
        let gone = DRAWN
            .with_borrow_mut(|d| d.insert(&owner, key, Kept { bitmap: copy, hash }, pixels, now()));
        close(gone);
        trim_later();
    });
}

/// Drawings taken from the cache since the last call.
pub(crate) fn take_reused() -> u32 {
    REUSED.with(|r| r.replace(0))
}

/// Lets the drawings kept go once unused long enough: a timer firing as
/// each would expire, until none is left.
fn trim_later() {
    if TRIM.with_borrow(Option::is_some) {
        return;
    }
    let timeout = Timeout::new((RECENT_SECONDS * 1000.0) as u32 + 1000, || {
        TRIM.with_borrow_mut(|t| *t = None);
        let (gone, left) = DRAWN.with_borrow_mut(|d| (d.trim(now()), d.len()));
        close(gone);
        if left > 0 {
            trim_later();
        }
    });
    TRIM.with_borrow_mut(|t| *t = Some(timeout));
}

/// Lets every drawing and part kept go (a skin deleted).
pub(crate) fn forget() {
    close(DRAWN.with_borrow_mut(RecentImages::clear));
    PARTS.with_borrow_mut(HashMap::clear);
}

/// `ddi_noteskin::component::part_documents` of the file at `path` whose
/// text is `text`, read once.
pub(crate) fn part_documents(path: &str, text: &str) -> Result<PartDocuments, String> {
    let key = (
        path.to_string(),
        ddi_noteskin::report::hash(text.as_bytes()),
    );
    if let Some(parts) = PARTS.with_borrow(|p| p.get(&key).cloned()) {
        return Ok(parts);
    }
    let parts = ddi_noteskin::component::part_documents(path, text)?;
    PARTS.with_borrow_mut(|p| {
        if p.len() >= MAX_PARTS {
            p.clear();
        }
        p.insert(key, parts.clone());
    });
    Ok(parts)
}

/// A load of a skin under way ([`hold`]); dropped, the next may go.
pub(crate) struct Holding(String);

/// Waits for the loads of skin `key` under way to end (at most
/// [`LOAD_WAIT_MS`]: then it loads anyway), and holds it.
pub(crate) async fn hold(key: &str) -> Holding {
    let deadline = js_sys::Date::now() + f64::from(LOAD_WAIT_MS);
    loop {
        let left = deadline - js_sys::Date::now();
        let busy = LOADING.with_borrow(|l| l.get(key).is_some_and(|(n, _)| *n > 0));
        if !busy || left <= 0.0 {
            break;
        }
        let key = key.to_string();
        let promise = Promise::new(&mut |resolve, _| {
            LOADING.with_borrow_mut(|l| {
                if let Some((_, waiting)) = l.get_mut(&key) {
                    waiting.push(resolve.clone());
                }
            });
            let resolve = resolve.clone();
            Timeout::new(left.ceil() as u32, move || {
                let _ = resolve.call0(&JsValue::NULL);
            })
            .forget();
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
    LOADING.with_borrow_mut(|l| l.entry(key.to_string()).or_default().0 += 1);
    Holding(key.to_string())
}

impl Drop for Holding {
    fn drop(&mut self) {
        let waiting = LOADING.with_borrow_mut(|l| {
            let (n, _) = l.get_mut(&self.0)?;
            *n = n.saturating_sub(1);
            if *n > 0 {
                return None;
            }
            l.remove(&self.0).map(|(_, w)| w)
        });
        for resolve in waiting.into_iter().flatten() {
            let _ = resolve.call0(&JsValue::NULL);
        }
    }
}
