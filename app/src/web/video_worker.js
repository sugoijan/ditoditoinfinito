// The video decoder worker (docs/plans/video-backgrounds.md, step 2). Runs
// the FFmpeg module (`video/ddivideo.wasm`, built by video/build.sh), which
// is a WASI reactor: this file implements the WASI calls it imports, over
// the `Blob`s the page sends, read with `FileReaderSync`. Embedded in the
// app (`video.rs`) and started from a blob URL, so it always matches the
// app's protocol; the module is fetched on the first `open`.
//
// Page → worker: {type: "init", base} (URL of the module's folder), then
//   {type: "open", id, blob, name}, {type: "seek", id, t, gen},
//   {type: "want", id, n, gen}, {type: "close", id}.
// Worker → page: {type: "loading", loaded, total}, {type: "ready", ms,
//   version}, {type: "failed", message} (the module), {type: "opened", id,
//   width, height, duration, codec}, {type: "frame", id, gen, pts, width,
//   height, colorspace, full_range, decode_ms, planes} (planes transferred),
//   {type: "eof", id, gen, last}, {type: "error", id, message}.
// Messages are handled one at a time, in order; frames are decoded only on
// `want`, so the page sets the queue depth. A seek cancels the wants from
// before it (they carry the old `gen`), and so does the end of the movie:
// `eof` is posted once, until the next seek.
"use strict";

// WASI preview1 values.
const ERRNO = { SUCCESS: 0, BADF: 8, INVAL: 28, IO: 29, ISDIR: 31, NOENT: 44, NOSYS: 52, NOTDIR: 54, ROFS: 69, SPIPE: 70 };
const FILETYPE = { CHARACTER_DEVICE: 2, DIRECTORY: 3, REGULAR_FILE: 4 };
const OFLAGS = { CREAT: 1, DIRECTORY: 2, EXCL: 4, TRUNC: 8 };
const PREOPEN = "/v"; // the module opens `/v/<name>`
const PREOPEN_FD = 3;
const READ_AHEAD = 64 * 1024;
const MAX_LOG_LINES = 200;
// Consecutive frames that fail to decode before the movie counts as broken.
const MAX_BAD_FRAMES = 50;

let base = null;
let compiled = null; // the compiled module, kept to restart after a crash
let loading = null; // promise of the compiled module
let exports = null; // the instance's exports, its functions wrapped by `guarded`
const files = new Map(); // file name in /v → Blob
const fds = new Map(); // fd → {kind, blob, size, pos, cache, cacheAt}
let nextFd = PREOPEN_FD + 1;
const videos = new Map(); // id → {ptr, name, gen, last, bad}
let logLines = 0;
const logBuffers = { 1: "", 2: "" };
const encoder = new TextEncoder();
const decoder = new TextDecoder();

const post = (msg, transfer) => self.postMessage(msg, transfer || []);

// --- WASI ---------------------------------------------------------------

const view = () => new DataView(exports.memory.buffer);
const bytes = () => new Uint8Array(exports.memory.buffer);
const readString = (ptr, len) => decoder.decode(bytes().subarray(ptr, ptr + len));

function log(fd, text) {
  logBuffers[fd] += text;
  let nl;
  while ((nl = logBuffers[fd].indexOf("\n")) >= 0) {
    const line = logBuffers[fd].slice(0, nl);
    logBuffers[fd] = logBuffers[fd].slice(nl + 1);
    logLines++;
    if (logLines < MAX_LOG_LINES) console.warn("[video] " + line);
    else if (logLines === MAX_LOG_LINES) console.warn("[video] further decoder messages are not shown");
  }
}

// Bytes [pos, pos + n) of a file, through a read-ahead window.
function readAt(file, pos, n) {
  const end = Math.min(file.size, pos + n);
  if (pos >= end) return new Uint8Array(0);
  if (!file.cache || pos < file.cacheAt || end > file.cacheAt + file.cache.length) {
    const to = Math.min(file.size, pos + Math.max(n, READ_AHEAD));
    file.cache = new Uint8Array(new FileReaderSync().readAsArrayBuffer(file.blob.slice(pos, to)));
    file.cacheAt = pos;
  }
  return file.cache.subarray(pos - file.cacheAt, end - file.cacheAt);
}

function writeFilestat(ptr, filetype, size) {
  const v = view();
  for (let i = 0; i < 64; i += 8) v.setBigUint64(ptr + i, 0n, true);
  v.setUint8(ptr + 16, filetype);
  v.setBigUint64(ptr + 24, 1n, true); // nlink
  v.setBigUint64(ptr + 32, BigInt(size), true);
}

// The module's `abort()`; like a trap, it leaves the instance unusable.
class Exit extends Error {}

// Anything thrown out of the module (a trap, `abort()`, a stack overflow,
// which V8 reports as a RangeError, an exception from an import) leaves it
// in an unknown state.
class Crash extends Error {
  constructor(cause) {
    super(cause && cause.message ? cause.message : String(cause));
    this.cause = cause;
  }
}

function guarded(raw) {
  const wrapped = { memory: raw.memory };
  for (const [name, value] of Object.entries(raw)) {
    if (typeof value !== "function") continue;
    wrapped[name] = (...args) => {
      try {
        return value(...args);
      } catch (e) {
        throw new Crash(e);
      }
    };
  }
  return wrapped;
}

// The 20 calls the module imports (docs/plans/video-backgrounds.md, step 1).
const wasi = {
  environ_get: () => ERRNO.SUCCESS,
  environ_sizes_get: (count, size) => {
    view().setUint32(count, 0, true);
    view().setUint32(size, 0, true);
    return ERRNO.SUCCESS;
  },
  clock_time_get: (id, _precision, out) => {
    const ns = id === 0 ? BigInt(Date.now()) * 1000000n : BigInt(Math.round(performance.now() * 1e6));
    view().setBigUint64(out, ns, true);
    return ERRNO.SUCCESS;
  },
  fd_prestat_get: (fd, out) => {
    if (fd !== PREOPEN_FD) return ERRNO.BADF;
    view().setUint8(out, 0); // a directory
    view().setUint32(out + 4, PREOPEN.length, true);
    return ERRNO.SUCCESS;
  },
  fd_prestat_dir_name: (fd, ptr, len) => {
    if (fd !== PREOPEN_FD) return ERRNO.BADF;
    bytes().set(encoder.encode(PREOPEN).subarray(0, len), ptr);
    return ERRNO.SUCCESS;
  },
  fd_fdstat_get: (fd, out) => {
    let type;
    if (fd <= 2) type = FILETYPE.CHARACTER_DEVICE;
    else if (fd === PREOPEN_FD) type = FILETYPE.DIRECTORY;
    else if (fds.has(fd)) type = FILETYPE.REGULAR_FILE;
    else return ERRNO.BADF;
    const v = view();
    v.setUint8(out, type);
    v.setUint16(out + 2, 0, true);
    v.setBigUint64(out + 8, 0xffffffffffffffffn, true);
    v.setBigUint64(out + 16, 0xffffffffffffffffn, true);
    return ERRNO.SUCCESS;
  },
  fd_fdstat_set_flags: () => ERRNO.NOSYS,
  fd_filestat_get: (fd, out) => {
    if (fd === PREOPEN_FD) writeFilestat(out, FILETYPE.DIRECTORY, 0);
    else if (fds.has(fd)) writeFilestat(out, FILETYPE.REGULAR_FILE, fds.get(fd).size);
    else if (fd <= 2) writeFilestat(out, FILETYPE.CHARACTER_DEVICE, 0);
    else return ERRNO.BADF;
    return ERRNO.SUCCESS;
  },
  path_filestat_get: (fd, _flags, ptr, len, out) => {
    if (fd !== PREOPEN_FD) return ERRNO.BADF;
    const name = readString(ptr, len).replace(/^\.\//, "");
    if (name === "" || name === ".") writeFilestat(out, FILETYPE.DIRECTORY, 0);
    else if (files.has(name)) writeFilestat(out, FILETYPE.REGULAR_FILE, files.get(name).size);
    else return ERRNO.NOENT;
    return ERRNO.SUCCESS;
  },
  path_open: (fd, _dirflags, ptr, len, oflags, _rights, _inheriting, _fdflags, out) => {
    if (fd !== PREOPEN_FD) return ERRNO.BADF;
    const name = readString(ptr, len).replace(/^\.\//, "");
    if (oflags & (OFLAGS.CREAT | OFLAGS.EXCL | OFLAGS.TRUNC)) return ERRNO.ROFS;
    const blob = files.get(name);
    if (!blob) return ERRNO.NOENT;
    if (oflags & OFLAGS.DIRECTORY) return ERRNO.NOTDIR;
    const opened = nextFd++;
    fds.set(opened, { blob, size: blob.size, pos: 0, cache: null, cacheAt: 0 });
    view().setUint32(out, opened, true);
    return ERRNO.SUCCESS;
  },
  fd_read: (fd, iovs, count, out) => {
    const file = fds.get(fd);
    if (!file) return fd === PREOPEN_FD ? ERRNO.ISDIR : ERRNO.BADF;
    let total = 0;
    try {
      for (let i = 0; i < count; i++) {
        const v = view();
        const ptr = v.getUint32(iovs + i * 8, true);
        const len = v.getUint32(iovs + i * 8 + 4, true);
        const chunk = readAt(file, file.pos, len);
        bytes().set(chunk, ptr);
        file.pos += chunk.length;
        total += chunk.length;
        if (chunk.length < len) break;
      }
    } catch (e) {
      console.warn("[video] reading the file failed:", e);
      if (total === 0) return ERRNO.IO;
    }
    view().setUint32(out, total, true);
    return ERRNO.SUCCESS;
  },
  fd_seek: (fd, offset, whence, out) => {
    const file = fds.get(fd);
    if (!file) return fd <= 2 ? ERRNO.SPIPE : ERRNO.BADF;
    const from = [0, file.pos, file.size][whence];
    if (from === undefined) return ERRNO.INVAL;
    const pos = BigInt(from) + offset;
    if (pos < 0n || pos > BigInt(Number.MAX_SAFE_INTEGER)) return ERRNO.INVAL;
    file.pos = Number(pos);
    view().setBigUint64(out, pos, true);
    return ERRNO.SUCCESS;
  },
  fd_write: (fd, iovs, count, out) => {
    if (fd !== 1 && fd !== 2) return ERRNO.BADF;
    let total = 0;
    for (let i = 0; i < count; i++) {
      const v = view();
      const ptr = v.getUint32(iovs + i * 8, true);
      const len = v.getUint32(iovs + i * 8 + 4, true);
      log(fd, readString(ptr, len));
      total += len;
    }
    view().setUint32(out, total, true);
    return ERRNO.SUCCESS;
  },
  fd_close: (fd) => {
    if (!fds.delete(fd)) return ERRNO.BADF;
    return ERRNO.SUCCESS;
  },
  fd_readdir: () => ERRNO.NOSYS,
  path_remove_directory: () => ERRNO.NOSYS,
  path_rename: () => ERRNO.NOSYS,
  path_unlink_file: () => ERRNO.NOSYS,
  poll_oneoff: () => ERRNO.NOSYS,
  proc_exit: (code) => {
    throw new Exit("the video decoder exited (" + code + ")");
  },
};

// --- The module ---------------------------------------------------------

// Fetches and compiles the module once, reporting the download's progress.
// The description (`ddivideo.json`) is revalidated on every load and names
// the module by its hash, so a redeploy is never half old, half new.
function load() {
  if (!loading) {
    loading = (async () => {
      const start = performance.now();
      const describe = await fetch(new URL("ddivideo.json", base), { cache: "no-cache" });
      if (!describe.ok) {
        throw new Error(describe.status === 404 ? "the video decoder is not included in this build" : "loading the video decoder failed (HTTP " + describe.status + ")");
      }
      const info = await describe.json();
      const response = await fetch(new URL("ddivideo.wasm?v=" + String(info.sha256).slice(0, 16), base));
      if (!response.ok || !response.body) throw new Error("loading the video decoder failed (HTTP " + response.status + ")");
      // The body arrives decompressed, so the total is the module's own size.
      const total = info.size || 0;
      let loaded = 0;
      let reported = 0;
      const counted = response.body.pipeThrough(new TransformStream({
        transform(chunk, controller) {
          loaded += chunk.length;
          if (loaded - reported > 64 * 1024 || loaded >= total) {
            reported = loaded;
            post({ type: "loading", loaded, total });
          }
          controller.enqueue(chunk);
        },
      }));
      const module = await WebAssembly.compileStreaming(new Response(counted, { headers: { "Content-Type": "application/wasm" } }));
      post({ type: "ready", ms: performance.now() - start, version: info.version || "" });
      return module;
    })();
    // A failed download is tried again by the next `open`.
    loading.catch(() => {
      loading = null;
    });
  }
  return loading;
}

async function ensureInstance() {
  compiled = compiled || (await load());
  if (!exports) {
    const instance = await WebAssembly.instantiate(compiled, { wasi_snapshot_preview1: wasi });
    const wrapped = guarded(instance.exports);
    // Kept only once it initialised; `exports` must be set for the WASI
    // calls `_initialize` makes.
    exports = wrapped;
    try {
      wrapped._initialize();
    } catch (e) {
      exports = null;
      throw e;
    }
  }
}

// After a crash the module's memory cannot be trusted: every open movie
// fails (and `id`, the one being opened when it happened), and the next
// `open` starts a fresh instance.
function crashed(error, id) {
  const cause = error.cause || error;
  const message = "the video decoder crashed: " + (cause && cause.message ? cause.message : String(cause));
  console.warn("[video]", cause);
  for (const open of videos.keys()) post({ type: "error", id: open, message });
  if (id !== undefined && !videos.has(id)) post({ type: "error", id, message });
  videos.clear();
  files.clear();
  fds.clear();
  exports = null;
}

function cString(text) {
  const encoded = encoder.encode(text + "\0");
  const ptr = exports.ddi_malloc(encoded.length);
  bytes().set(encoded, ptr);
  return ptr;
}

function cStringAt(ptr) {
  const memory = bytes();
  let end = ptr;
  while (memory[end]) end++;
  return decoder.decode(memory.subarray(ptr, end));
}

// --- Messages -----------------------------------------------------------

async function open({ id, blob, name }) {
  try {
    await ensureInstance();
  } catch (e) {
    post({ type: "failed", message: e.message || String(e) });
    post({ type: "error", id, message: e.message || String(e) });
    return;
  }
  const extension = (/\.[A-Za-z0-9]{1,8}$/.exec(name || "") || [""])[0];
  const fileName = id + extension.toLowerCase();
  files.set(fileName, blob);
  const path = cString(PREOPEN + "/" + fileName);
  const ptr = exports.ddi_open(path);
  exports.ddi_free(path);
  if (!ptr) {
    files.delete(fileName);
    post({ type: "error", id, message: "cannot open " + name + " (not a movie the decoder knows)" });
    return;
  }
  videos.set(id, { ptr, fileName, gen: 0, last: -1, bad: 0, ended: false });
  const duration = exports.ddi_duration(ptr);
  post({
    type: "opened",
    id,
    width: exports.ddi_width(ptr),
    height: exports.ddi_height(ptr),
    duration: duration >= 0 ? duration : null,
    codec: cStringAt(exports.ddi_codec(ptr)),
  });
}

function want({ id, n, gen }) {
  const video = videos.get(id);
  if (!video || gen !== video.gen || video.ended) return;
  for (let i = 0; i < n; ) {
    const start = performance.now();
    const pts = exports.ddi_next(video.ptr);
    const ms = performance.now() - start;
    if (pts === -1) {
      video.ended = true;
      post({ type: "eof", id, gen: video.gen, last: video.last >= 0 ? video.last : null });
      return;
    }
    if (pts < 0) {
      if (++video.bad > MAX_BAD_FRAMES) {
        post({ type: "error", id, message: "the movie does not decode (" + MAX_BAD_FRAMES + " broken frames in a row)" });
        close({ id });
        return;
      }
      continue;
    }
    video.bad = 0;
    video.last = pts;
    const at = exports.ddi_planes(video.ptr);
    const planes = exports.memory.buffer.slice(at, at + exports.ddi_planes_size(video.ptr));
    post({
      type: "frame",
      id,
      gen: video.gen,
      pts,
      width: exports.ddi_width(video.ptr),
      height: exports.ddi_height(video.ptr),
      colorspace: exports.ddi_colorspace(video.ptr),
      full_range: exports.ddi_full_range(video.ptr) !== 0,
      decode_ms: ms,
      planes,
    }, [planes]);
    i++;
  }
}

function seek({ id, t, gen }) {
  const video = videos.get(id);
  if (!video) return;
  video.gen = gen;
  video.last = -1;
  video.bad = 0;
  video.ended = false;
  if (exports.ddi_seek(video.ptr, t) !== 0) {
    post({ type: "error", id, message: "the movie could not be read again after seeking" });
    close({ id });
  }
}

function close({ id }) {
  const video = videos.get(id);
  if (!video) return;
  videos.delete(id);
  exports.ddi_close(video.ptr);
  files.delete(video.fileName);
}

const handlers = { open, want, seek, close };
let queue = Promise.resolve();

self.onmessage = (event) => {
  const msg = event.data;
  if (msg.type === "init") {
    base = msg.base;
    return;
  }
  const handler = handlers[msg.type];
  if (!handler) return;
  queue = queue.then(async () => {
    // A movie whose open failed has no state; later messages are dropped.
    if (msg.type !== "open" && !videos.has(msg.id)) return;
    try {
      await handler(msg);
    } catch (e) {
      if (e instanceof Crash) crashed(e, msg.id);
      else if (msg.id !== undefined) post({ type: "error", id: msg.id, message: e.message || String(e) });
    }
  });
};
