//! Picked folders that can be read again (Chromium's File System Access
//! API): `showDirectoryPicker` returns a handle, which IndexedDB can keep,
//! so a pack folder can be re-read in place after it changed on disk.
//! Firefox and Safari have no such picker; there the folder input stays,
//! and a pack is updated by picking its folder again.
//!
//! Reading a kept handle again needs the player's permission, asked from a
//! click ([`access`]).

use js_sys::{Array, Promise};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::File;

use super::files::PickedFile;
use super::js_err;

#[wasm_bindgen(inline_js = r#"
export function ddiCanPickFolder() {
    return typeof window.showDirectoryPicker === "function";
}

// Resolves to the picked folder's handle, or null when the picker was
// dismissed. Call from a click: the picker needs the gesture.
export function ddiPickFolder() {
    return window.showDirectoryPicker({ id: "ddi-packs", mode: "read" }).catch((e) => {
        if (e && e.name === "AbortError") return null;
        throw e;
    });
}

// Whether the folder may be read, asking the player if needed. Call from a
// click: the permission prompt needs the gesture.
export async function ddiFolderAccess(handle) {
    const opts = { mode: "read" };
    if ((await handle.queryPermission(opts)) === "granted") return true;
    return (await handle.requestPermission(opts)) === "granted";
}

// Every file under the folder as [path, File], paths starting with the
// folder's own name like a folder input's `webkitRelativePath`.
export async function ddiReadFolder(handle) {
    const out = [];
    const walk = async (dir, prefix) => {
        for await (const [name, entry] of dir.entries()) {
            if (entry.kind === "file") {
                out.push([prefix + name, await entry.getFile()]);
            } else {
                await walk(entry, prefix + name + "/");
            }
        }
    };
    await walk(handle, handle.name + "/");
    return out;
}

export function ddiFolderName(handle) {
    return handle.name;
}

export function ddiSameFolder(a, b) {
    return a.isSameEntry(b);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = ddiCanPickFolder)]
    fn can_pick_folder() -> bool;
    #[wasm_bindgen(js_name = ddiPickFolder, catch)]
    fn pick_folder() -> Result<Promise, JsValue>;
    #[wasm_bindgen(js_name = ddiFolderAccess)]
    fn folder_access(handle: &JsValue) -> Promise;
    #[wasm_bindgen(js_name = ddiReadFolder)]
    fn read_folder(handle: &JsValue) -> Promise;
    #[wasm_bindgen(js_name = ddiFolderName)]
    fn folder_name(handle: &JsValue) -> String;
    #[wasm_bindgen(js_name = ddiSameFolder, catch)]
    fn same_folder(a: &JsValue, b: &JsValue) -> Result<Promise, JsValue>;
}

/// Whether folders can be picked as re-readable handles.
pub(crate) fn supported() -> bool {
    can_pick_folder()
}

/// Opens the folder picker. Call synchronously from a click; await the
/// result (`None` when dismissed).
pub(crate) fn pick() -> Result<impl Future<Output = Result<Option<JsValue>, String>>, String> {
    let promise = pick_folder().map_err(|e| js_err("folder picker", e))?;
    Ok(async move {
        let handle = JsFuture::from(promise)
            .await
            .map_err(|e| js_err("folder picker", e))?;
        Ok((!handle.is_null() && !handle.is_undefined()).then_some(handle))
    })
}

/// Asks to read a kept folder again. Call synchronously from a click.
pub(crate) fn access(handle: &JsValue) -> impl Future<Output = bool> + use<> {
    let promise = folder_access(handle);
    async move {
        JsFuture::from(promise)
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

/// The folder's name.
pub(crate) fn name(handle: &JsValue) -> String {
    folder_name(handle)
}

/// Whether two handles are the same folder on disk.
pub(crate) async fn same(a: &JsValue, b: &JsValue) -> bool {
    let Ok(p) = same_folder(a, b) else {
        return false;
    };
    JsFuture::from(p)
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Every file under the folder, as a folder input would list them.
pub(crate) async fn read(handle: &JsValue) -> Result<Vec<PickedFile>, String> {
    let list = JsFuture::from(read_folder(handle))
        .await
        .map_err(|e| js_err("reading the folder", e))?;
    Ok(Array::from(&list)
        .iter()
        .filter_map(|pair| {
            let pair = Array::from(&pair);
            let path = pair.get(0).as_string()?;
            let file = pair.get(1).dyn_into::<File>().ok()?;
            Some(PickedFile { path, file })
        })
        .collect())
}
