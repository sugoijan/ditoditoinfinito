//! Songs shared by link (`docs/plans/shared-songs.md`): a share read from
//! the private Worker at [`base`] by its id (`#/s/<id>`), held in memory
//! for the session and never in the library or IndexedDB; its songs are
//! the records the web stores imported songs as, their files reached by
//! URL. One share is current at a time ([`current`]); a play of one of
//! its songs, opened at its own address, loads the share again by the id
//! in the song's id ([`song_id`], `shared:<id>:<n>`).

use std::cell::RefCell;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::songs::ImportedSong;

/// The Worker's base URL: the build's (`DDI_SHARE_URL`), else the
/// production one; `window.__DDI_SHARE_URL` overrides it for tests.
pub(crate) fn base() -> String {
    let from_page = web_sys::window().and_then(|w| {
        js_sys::Reflect::get(&w, &"__DDI_SHARE_URL".into())
            .ok()
            .and_then(|v| v.as_string())
    });
    from_page
        .unwrap_or_else(|| {
            option_env!("DDI_SHARE_URL")
                .unwrap_or("https://sugoijan.dev/ddi-share")
                .to_string()
        })
        .trim_end_matches('/')
        .to_string()
}

/// A share as the Worker answers `GET <base>/s/<id>`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Share {
    #[serde(default)]
    pub(crate) pack: String,
    /// Unix seconds; `None`: never.
    #[serde(default)]
    pub(crate) expires_at: Option<f64>,
    #[serde(default)]
    pub(crate) created_at: f64,
    pub(crate) songs: Vec<SharedSong>,
    #[serde(default)]
    pub(crate) pack_art: Option<String>,
    /// The share's id, as loaded (not in the Worker's answer).
    #[serde(skip)]
    pub(crate) id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SharedSong {
    pub(crate) record: ImportedSong,
    #[serde(default)]
    pub(crate) files: Vec<SharedFile>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SharedFile {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) bytes: f64,
    pub(crate) url: String,
}

/// Why a share could not be read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShareError {
    /// No share has this id (404).
    Missing,
    /// It had, and expired (410).
    Expired,
    Other(String),
}

impl ShareError {
    /// What the page says: one line for a link that is gone either way.
    pub(crate) fn message(&self) -> String {
        match self {
            ShareError::Missing | ShareError::Expired => {
                "This link doesn't exist or has expired.".into()
            }
            ShareError::Other(e) => format!("The shared songs could not be loaded: {e}"),
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Share>>> = const { RefCell::new(None) };
}

/// The share loaded last, if any.
pub(crate) fn current() -> Option<Rc<Share>> {
    CURRENT.with_borrow(Clone::clone)
}

pub(crate) use ddi_library::share::{is_shared_id, parse_song_id, song_id, valid_id};

/// The URL of file `path` of the shared song `song_id`, as the share
/// lists it (a file it does not list is asked for at the Worker's path
/// for it, which also serves what the uploader sent and the list left
/// out); `None` for a song of no share.
pub(crate) fn file_url(song_id: &str, path: &str) -> Option<String> {
    let (share, n) = parse_song_id(song_id)?;
    if let Some(s) = current().filter(|s| s.id == share)
        && let Some(song) = s.songs.get(n)
        && let Some(f) = song.files.iter().find(|f| f.path == path)
    {
        return Some(f.url.clone());
    }
    let encoded: Vec<String> = path
        .split('/')
        .map(|p| String::from(js_sys::encode_uri_component(p)))
        .collect();
    Some(format!("{}/s/{share}/f/{n}/{}", base(), encoded.join("/")))
}

/// The share `id`, as loaded now or read from the Worker and kept as
/// the current one: each song's record renamed to its shared id, with
/// nothing of the library it came from (no kept folder, no links, no
/// opening still).
pub(crate) async fn load(id: &str) -> Result<Rc<Share>, ShareError> {
    if !valid_id(id) {
        return Err(ShareError::Missing);
    }
    if let Some(s) = current().filter(|s| s.id == id) {
        return Ok(s);
    }
    publish(id, "loading", None);
    let url = format!("{}/s/{id}", base());
    let resp = gloo::net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| ShareError::Other(format!("GET {url}: {e}")))?;
    let result = match resp.status() {
        404 => Err(ShareError::Missing),
        410 => Err(ShareError::Expired),
        s if !resp.ok() => Err(ShareError::Other(format!("GET {url}: HTTP {s}"))),
        _ => resp
            .text()
            .await
            .map_err(|e| ShareError::Other(e.to_string()))
            .and_then(|t| from_json(id, &t)),
    };
    match &result {
        Ok(share) => {
            let share = share.clone();
            CURRENT.with_borrow_mut(|c| *c = Some(share.clone()));
            publish(id, "ready", Some(&share));
        }
        Err(e) => publish(
            id,
            match e {
                ShareError::Missing => "missing",
                ShareError::Expired => "expired",
                ShareError::Other(_) => "failed",
            },
            None,
        ),
    }
    result
}

/// A share from the Worker's JSON, its songs made this share's.
pub(crate) fn from_json(id: &str, text: &str) -> Result<Rc<Share>, ShareError> {
    let mut share: Share =
        serde_json::from_str(text).map_err(|e| ShareError::Other(format!("the share: {e}")))?;
    share.id = id.to_string();
    for (n, song) in share.songs.iter_mut().enumerate() {
        let r = &mut song.record;
        r.entry.id = song_id(id, n);
        r.folder = None;
        r.links.clear();
        r.still = None;
        if r.pack.is_empty() {
            r.pack = share.pack.clone();
        }
    }
    Ok(Rc::new(share))
}

impl Share {
    /// How long it has left, in a few words; `None`: forever.
    pub(crate) fn expires_in(&self, now_secs: f64) -> Option<String> {
        ddi_library::share::expires_in(self.expires_at, now_secs)
    }
}

/// `window.__DDI_SHARE`, for tests (its own object: the UI host writes
/// `__DDI_DEBUG` whole each frame): `{id, state, pack, songs,
/// expiresAt, expiresIn}`, `state` among `loading`, `ready`, `missing`,
/// `expired`, `failed`.
fn publish(id: &str, state: &str, share: Option<&Share>) {
    let Some(w) = web_sys::window() else {
        return;
    };
    let o = js_sys::Object::new();
    let set = |k: &str, v: wasm_bindgen::JsValue| {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v);
    };
    set("id", id.into());
    set("state", state.into());
    if let Some(s) = share {
        set("pack", s.pack.as_str().into());
        set("songs", (s.songs.len() as f64).into());
        set(
            "expiresAt",
            s.expires_at
                .map_or(wasm_bindgen::JsValue::NULL, wasm_bindgen::JsValue::from_f64),
        );
        let now = js_sys::Date::now() / 1000.0;
        set(
            "expiresIn",
            s.expires_in(now)
                .map_or(wasm_bindgen::JsValue::NULL, |t| t.into()),
        );
    }
    let _ = js_sys::Reflect::set(&w, &"__DDI_SHARE".into(), &o);
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn a_share_reads_from_the_workers_json_and_takes_its_songs() {
        let text = r#"{"pack":"A pack","expires_at":1900000000,"created_at":1700000000,
            "songs":[{"record":{"entry":{"id":"u-1","title":"T","artist":"A","chart":"a.ssc","music":"a.ogg"},
            "pack":"","folder":"folders/x","links":{"a.ogg":{"path":"a.ogg"}},
            "still":{"state":"no_movie"}},
            "files":[{"path":"a.ssc","bytes":10,"url":"https://w/s/abc12345/f/0/a.ssc"}]}]}"#;
        let share = from_json("abc12345", text).expect("a share");
        assert_eq!(share.id, "abc12345");
        assert_eq!(share.pack, "A pack");
        let r = &share.songs[0].record;
        assert_eq!(r.entry.id, "shared:abc12345:0");
        assert_eq!(r.pack, "A pack");
        assert!(r.folder.is_none() && r.links.is_empty() && r.still.is_none());
        assert_eq!(
            share.songs[0].files[0].url,
            "https://w/s/abc12345/f/0/a.ssc"
        );
        assert!(from_json("abc12345", "{").is_err());
    }
}
