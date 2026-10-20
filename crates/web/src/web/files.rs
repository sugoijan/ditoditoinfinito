//! Getting user files in: `<input type="file">` (with or without
//! `webkitdirectory`), drag and drop of files and folders, and reading
//! `Blob`s back. Paths are `/`-separated and relative to what the user
//! picked, e.g. `My Pack/Song/song.ssc`.

use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, BlobPropertyBag, DataTransfer, File, FileList, FileSystemDirectoryEntry, FileSystemEntry,
    FileSystemFileEntry, HtmlInputElement,
};

use super::js_err;

/// A picked file and its path relative to the picked root.
#[derive(Clone)]
pub(crate) struct PickedFile {
    pub(crate) path: String,
    pub(crate) file: File,
}

pub(crate) async fn blob_bytes(blob: &Blob) -> Result<Vec<u8>, String> {
    let buffer = JsFuture::from(blob.array_buffer())
        .await
        .map_err(|e| js_err("reading a file", e))?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

/// `URL.createObjectURL`; pair with [`revoke_object_url`].
pub(crate) fn object_url(blob: &Blob) -> Option<String> {
    web_sys::Url::create_object_url_with_blob(blob).ok()
}

pub(crate) fn revoke_object_url(url: &str) {
    let _ = web_sys::Url::revoke_object_url(url);
}

/// Wraps bytes produced in wasm (an inflated zip entry) in a `Blob` of the
/// given content type (empty for none).
pub(crate) fn bytes_blob(bytes: &[u8], mime: &str) -> Result<Blob, String> {
    let parts = Array::of1(&Uint8Array::from(bytes));
    let options = BlobPropertyBag::new();
    options.set_type(mime);
    Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(|e| js_err("Blob", e))
}

/// Files chosen in an `<input type="file">`. With `webkitdirectory` every
/// file carries its path below the chosen folder (`webkitRelativePath`,
/// which includes the folder's own name); plain picks use the file name.
pub(crate) fn from_input(input: &HtmlInputElement) -> Vec<PickedFile> {
    input
        .files()
        .map(|l| from_file_list(&l))
        .unwrap_or_default()
}

fn from_file_list(list: &FileList) -> Vec<PickedFile> {
    (0..list.length())
        .filter_map(|i| list.get(i))
        .map(|file| {
            let relative = Reflect::get(&file, &JsValue::from_str("webkitRelativePath"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            let path = if relative.is_empty() {
                file.name()
            } else {
                relative
            };
            PickedFile { path, file }
        })
        .collect()
}

/// Top-level entries of a drop. Must be called synchronously inside the
/// `drop` handler: the data transfer is emptied once the handler returns.
pub(crate) fn drop_entries(dt: &DataTransfer) -> DropEntries {
    let items = dt.items();
    let entries: Vec<FileSystemEntry> = (0..items.length())
        .filter_map(|i| items.get(i))
        .filter(|item| item.kind() == "file")
        .filter_map(|item| item.webkit_get_as_entry().ok().flatten())
        .collect();
    if entries.is_empty() {
        // No entry API (or not files): plain file list, no folders.
        return DropEntries::Files(dt.files().map(|l| from_file_list(&l)).unwrap_or_default());
    }
    DropEntries::Entries(entries)
}

pub(crate) enum DropEntries {
    Entries(Vec<FileSystemEntry>),
    Files(Vec<PickedFile>),
}

impl DropEntries {
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            DropEntries::Entries(e) => e.is_empty(),
            DropEntries::Files(f) => f.is_empty(),
        }
    }

    /// Walks dropped folders recursively.
    pub(crate) async fn collect(self) -> Result<Vec<PickedFile>, String> {
        let entries = match self {
            DropEntries::Files(files) => return Ok(files),
            DropEntries::Entries(entries) => entries,
        };
        let mut out = Vec::new();
        // Depth-first with an explicit stack: (entry, path of its parent).
        let mut stack: Vec<(FileSystemEntry, String)> = entries
            .into_iter()
            .rev()
            .map(|e| (e, String::new()))
            .collect();
        while let Some((entry, parent)) = stack.pop() {
            let path = if parent.is_empty() {
                entry.name()
            } else {
                format!("{parent}/{}", entry.name())
            };
            if entry.is_directory() {
                let dir: FileSystemDirectoryEntry = entry.unchecked_into();
                let children = read_directory(&dir).await?;
                stack.extend(children.into_iter().rev().map(|c| (c, path.clone())));
            } else if entry.is_file() {
                let file = entry_file(entry.unchecked_ref()).await?;
                out.push(PickedFile { path, file });
            }
        }
        Ok(out)
    }
}

/// Runs a callback-style API as a future. The callbacks free themselves
/// when called; the one that never fires stays allocated, which is a few
/// bytes per directory or file read.
async fn callback_future(
    call: impl FnOnce(&Function, &Function) -> Result<(), JsValue>,
) -> Result<JsValue, JsValue> {
    let mut issued = Ok(());
    let mut call = Some(call);
    let promise = Promise::new(&mut |resolve: Function, reject: Function| {
        let ok = Closure::once_into_js(move |v: JsValue| {
            let _ = resolve.call1(&JsValue::NULL, &v);
        });
        let fail = Closure::once_into_js(move |e: JsValue| {
            let _ = reject.call1(&JsValue::NULL, &e);
        });
        if let Some(call) = call.take() {
            issued = call(ok.unchecked_ref(), fail.unchecked_ref());
        }
    });
    issued?;
    JsFuture::from(promise).await
}

/// `readEntries` returns at most ~100 entries per call; repeat until empty.
async fn read_directory(dir: &FileSystemDirectoryEntry) -> Result<Vec<FileSystemEntry>, String> {
    let reader = dir.create_reader();
    let mut out = Vec::new();
    loop {
        let batch =
            callback_future(|ok, fail| reader.read_entries_with_callback_and_callback(ok, fail))
                .await
                .map_err(|e| js_err("reading a dropped folder", e))?;
        let batch: Array = batch.unchecked_into();
        if batch.length() == 0 {
            return Ok(out);
        }
        out.extend(batch.iter().map(|e| e.unchecked_into::<FileSystemEntry>()));
    }
}

async fn entry_file(entry: &FileSystemFileEntry) -> Result<File, String> {
    callback_future(|ok, fail| {
        entry.file_with_callback_and_callback(ok, fail);
        Ok(())
    })
    .await
    .map(|f| f.unchecked_into())
    .map_err(|e| js_err("reading a dropped file", e))
}
