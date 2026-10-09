// Checks the video decoder module under Node's WASI (Node 22 or later):
// opens a file, decodes it, prints the time per frame, checks that
// timestamps increase and that seeking lands on the same frames as decoding
// straight through, and optionally compares every frame with a raw YUV 4:2:0
// reference (`ffmpeg -i <file> -fps_mode passthrough -pix_fmt yuv420p
// -f rawvideo ref.yuv`) and writes one frame as PNG. Exits non-zero when a
// check fails.
//
//   node video/check.mjs <file> [--frames N] [--png out.png] [--png-frame K]
//                        [--reference ref.yuv] [--min-psnr dB]
//                        [--colorspace 601|709] [--range limited|full] [--wasm path]
import { WASI } from 'node:wasi';
import { readFile, writeFile } from 'node:fs/promises';
import { openSync, readSync, closeSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { deflateSync, crc32 } from 'node:zlib';
import { parseArgs } from 'node:util';

const { values: opt, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    wasm: { type: 'string', default: resolve(import.meta.dirname, '../target/video/ddivideo.wasm') },
    frames: { type: 'string' },
    png: { type: 'string' },
    'png-frame': { type: 'string' },
    reference: { type: 'string' },
    'min-psnr': { type: 'string', default: '40' },
    colorspace: { type: 'string' },
    range: { type: 'string' },
  },
});
if (positionals.length !== 1) {
  console.error('usage: node video/check.mjs <file> [--frames N] [--png out.png] [--png-frame K] [--reference ref.yuv] [--min-psnr dB] [--colorspace 601|709] [--range limited|full] [--wasm path]');
  process.exit(2);
}
const file = resolve(positionals[0]);
const maxFrames = opt.frames ? Number(opt.frames) : Infinity;
const failures = [];
const fail = (msg) => { failures.push(msg); console.log(`  FAIL ${msg}`); };

const wasi = new WASI({ version: 'preview1', preopens: { '/v': dirname(file) }, args: [], env: {} });
const { instance } = await WebAssembly.instantiate(await readFile(opt.wasm), {
  wasi_snapshot_preview1: wasi.wasiImport,
});
wasi.initialize(instance); // a reactor: `_initialize`, not `_start`
const e = instance.exports;
const mem = () => new Uint8Array(e.memory.buffer); // memory may grow on any call
const cstring = (s) => {
  const bytes = Buffer.from(s + '\0');
  const p = e.ddi_malloc(bytes.length);
  mem().set(bytes, p);
  return p;
};
const readCString = (p) => {
  const m = mem();
  let end = p;
  while (m[end]) end++;
  return Buffer.from(m.subarray(p, end)).toString();
};

const t0 = performance.now();
const path = cstring('/v/' + basename(file));
const v = e.ddi_open(path);
e.ddi_free(path);
if (!v) {
  console.log(`${basename(file)}: open failed`);
  process.exit(1);
}
const opened = performance.now() - t0;
const duration = e.ddi_duration(v);
console.log(`${basename(file)}: ${readCString(e.ddi_codec(v))} ${e.ddi_width(v)}x${e.ddi_height(v)}, ` +
  `duration ${duration < 0 ? 'unknown' : duration.toFixed(3) + ' s'}, opened in ${opened.toFixed(0)} ms`);

// The frame just decoded: its planes, copied out of wasm memory.
const planes = () => {
  const p = e.ddi_planes(v);
  return Buffer.from(mem().subarray(p, p + e.ddi_planes_size(v)));
};

// Straight through.
const pts = [], crcs = [];
let errors = 0, keep = null, decodeMs = 0;
const pngFrame = opt['png-frame'] !== undefined ? Number(opt['png-frame']) : null;
const ref = opt.reference ? openSync(opt.reference, 'r') : null;
const psnr = { y: Infinity, u: Infinity, v: Infinity };
let refFrames = 0;
for (;;) {
  const start = performance.now();
  const t = e.ddi_next(v);
  decodeMs += performance.now() - start;
  if (t === -1) break;
  if (t < 0) {
    errors++;
    if (errors > 100) { fail('more than 100 decode errors'); break; }
    continue;
  }
  const n = pts.length;
  if (n > 0 && !(t > pts[n - 1])) fail(`frame ${n} at ${t.toFixed(4)} s is not after frame ${n - 1} at ${pts[n - 1].toFixed(4)} s`);
  const frame = planes();
  pts.push(t);
  crcs.push(crc32(frame));
  const w = e.ddi_width(v), h = e.ddi_height(v);
  if (n === (pngFrame ?? 0) || (pngFrame === null && n === 30)) {
    keep = { n, t, w, h, planes: frame, colorspace: e.ddi_colorspace(v), full: e.ddi_full_range(v) };
  }
  if (ref) {
    const expected = Buffer.alloc(frame.length);
    if (readSync(ref, expected, 0, expected.length, n * expected.length) === expected.length) {
      refFrames++;
      const cw = (w + 1) >> 1, ch = (h + 1) >> 1;
      const parts = { y: [0, w * h], u: [w * h, w * h + cw * ch], v: [w * h + cw * ch, frame.length] };
      for (const [k, [a, b]] of Object.entries(parts)) psnr[k] = Math.min(psnr[k], planePsnr(frame, expected, a, b));
    }
  }
  if (pts.length >= maxFrames) break;
}
if (ref) closeSync(ref);
const n = pts.length;
if (n === 0) fail('no frame decoded');
else {
  console.log(`  ${n} frames, ${(decodeMs / n).toFixed(2)} ms/frame, pts ${pts[0].toFixed(3)}..${pts[n - 1].toFixed(3)} s, ` +
    `errors ${errors}, BT.${e.ddi_colorspace(v)} ${e.ddi_full_range(v) ? 'full' : 'limited'} range`);
}

const colorspace = e.ddi_colorspace(v), range = e.ddi_full_range(v) ? 'full' : 'limited';
if (opt.colorspace && Number(opt.colorspace) !== colorspace) fail(`BT.${colorspace}, expected BT.${opt.colorspace}`);
if (opt.range && opt.range !== range) fail(`${range} range, expected ${opt.range}`);

if (ref) {
  const fmt = (x) => (x === Infinity ? 'exact' : x.toFixed(1) + ' dB');
  console.log(`  against the reference (${refFrames} frames): worst PSNR Y ${fmt(psnr.y)}, U ${fmt(psnr.u)}, V ${fmt(psnr.v)}`);
  const min = Number(opt['min-psnr']);
  if (refFrames !== n) fail(`reference has ${refFrames} frames where ${n} were decoded`);
  for (const k of ['y', 'u', 'v']) if (psnr[k] < min) fail(`plane ${k.toUpperCase()} below ${min} dB`);
  if (errors > 0) fail(`${errors} decode errors`);
}

// Seeking: to a frame's own time or the middle of its period must give the
// frame (byte for byte) that decoding straight through gave; past the end,
// the end. Every frame on short clips, about 60 spread over long files.
const seekTo = (t, expect) => {
  if (e.ddi_seek(v, t) !== 0) return fail(`seek to ${t.toFixed(3)} s failed`), 0;
  const start = performance.now();
  const got = e.ddi_next(v);
  const ms = performance.now() - start;
  const label = `seek to ${t.toFixed(4)} s`;
  if (expect === null) {
    if (got !== -1) fail(`${label} (past the end) gave a frame at ${got}`);
  } else if (Math.abs(got - pts[expect]) > 1e-6) {
    fail(`${label} gave the frame at ${got.toFixed(4)} s, expected frame ${expect} at ${pts[expect].toFixed(4)} s`);
  } else if (crc32(planes()) !== crcs[expect]) {
    fail(`${label} gave frame ${expect} with different pixels`);
  }
  return ms;
};
if (n > 1) {
  const step = Math.max(1, Math.floor(n / 60));
  const targets = [[0, 0], [pts[n - 1], n - 1]];
  for (let i = 0; i < n; i += step) {
    targets.push([pts[i], i]);
    if (i + 1 < n) targets.push([(pts[i] + pts[i + 1]) / 2, i]);
  }
  let worst = 0;
  const before = failures.length;
  for (const [t, i] of targets) worst = Math.max(worst, seekTo(t, i));
  if (n < maxFrames) seekTo(pts[n - 1] + 10, null);
  if (failures.length === before) console.log(`  ${targets.length} seeks landed on the frames decoded straight through (slowest ${worst.toFixed(0)} ms)`);
}

if (keep && opt.png) {
  await writeFile(opt.png, png(keep.w, keep.h, toRgb(keep)));
  console.log(`  frame ${keep.n} (${keep.t.toFixed(3)} s) written to ${opt.png}`);
}
e.ddi_close(v);
process.exit(failures.length ? 1 : 0);

function planePsnr(a, b, from, to) {
  let se = 0;
  for (let i = from; i < to; i++) { const d = a[i] - b[i]; se += d * d; }
  return se === 0 ? Infinity : 10 * Math.log10((255 * 255 * (to - from)) / se);
}

// YUV 4:2:0 to RGB with the frame's matrix and range (what the background
// shader does).
function toRgb({ w, h, planes, colorspace, full }) {
  const [kr, kb] = colorspace === 709 ? [0.2126, 0.0722] : [0.299, 0.114];
  const kg = 1 - kr - kb;
  const cw = (w + 1) >> 1, ch = (h + 1) >> 1;
  const u0 = w * h, v0 = u0 + cw * ch;
  const rgb = Buffer.alloc(w * h * 3);
  const clamp = (x) => (x < 0 ? 0 : x > 255 ? 255 : Math.round(x));
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const c = (y >> 1) * cw + (x >> 1);
      let Y = planes[y * w + x], U = planes[u0 + c] - 128, V = planes[v0 + c] - 128;
      if (!full) { Y = (Y - 16) * (255 / 219); U *= 255 / 224; V *= 255 / 224; }
      const r = Y + 2 * (1 - kr) * V, b = Y + 2 * (1 - kb) * U, g = (Y - kr * r - kb * b) / kg;
      const o = (y * w + x) * 3;
      rgb[o] = clamp(r); rgb[o + 1] = clamp(g); rgb[o + 2] = clamp(b);
    }
  }
  return rgb;
}

function png(w, h, rgb) {
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++) rgb.copy(raw, y * (w * 3 + 1) + 1, y * w * 3, (y + 1) * w * 3);
  const chunk = (type, data) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(data.length);
    head.write(type, 4);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])));
    return Buffer.concat([head, data, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2; // 8-bit RGB
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
