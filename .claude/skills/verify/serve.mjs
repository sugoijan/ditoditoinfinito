// A static server for a test bundle: node serve.mjs <dir> <port> [--share <sharedir>] (127.0.0.1).
// `python3 -m http.server` resets connections under the page's parallel
// requests (Chrome then shows "This browser cannot run …"); this does not.
// With --share, it stands in for the shared-songs Worker under /share
// (`docs/plans/shared-songs.md`): GET /share/s/<id> answers
// <sharedir>/<id>/share.json (404 without it, 410 with a file `expired`
// beside it), GET /share/s/<id>/f/<n>/<path> a file at <sharedir>/<id>/<n>/<path>
// with range requests; the page sets `window.__DDI_SHARE_URL` to it.
// The uploader's side too, with the token `test-token`: POST /share/s
// makes <sharedir>/<id>/ (the id random, `share.json` written from the
// request with `expires_at` from `expires`, and a file `pending` until
// POST /share/s/<id>/finish removes it; GET answers 404 meanwhile), PUT
// /share/s/<id>/f/<n>/<path> and /share/s/<id>/art write the files,
// DELETE /share/s/<id> removes the folder.
import { createServer } from 'node:http';
import { createReadStream, statSync, existsSync, mkdirSync, writeFileSync, rmSync, unlinkSync } from 'node:fs';
import { join, extname, normalize, dirname } from 'node:path';
import { randomBytes } from 'node:crypto';
const args = process.argv.slice(2);
const shareAt = args.indexOf('--share');
const shareDir = shareAt >= 0 ? args[shareAt + 1] : null;
const [dir, port] = args.filter((_, i) => shareAt < 0 || (i !== shareAt && i !== shareAt + 1));
const types = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.opus': 'audio/ogg', '.ogg': 'audio/ogg', '.mp3': 'audio/mpeg', '.mp4': 'video/mp4', '.avi': 'video/x-msvideo', '.lua': 'text/plain', '.ini': 'text/plain', '.txt': 'text/plain', '.ssc': 'text/plain', '.sm': 'text/plain' };
const serveFile = (req, res, file) => {
  const st = statSync(file);
  if (!st.isFile()) throw new Error();
  const type = types[extname(file).toLowerCase()] || 'application/octet-stream';
  const range = /^bytes=(\d*)-(\d*)$/.exec(req.headers.range || '');
  if (range && (range[1] || range[2])) {
    const start = range[1] ? Number(range[1]) : Math.max(0, st.size - Number(range[2]));
    const end = range[1] && range[2] ? Math.min(Number(range[2]), st.size - 1) : st.size - 1;
    res.writeHead(206, { 'content-type': type, 'content-length': end - start + 1, 'content-range': `bytes ${start}-${end}/${st.size}`, 'accept-ranges': 'bytes' });
    createReadStream(file, { start, end }).pipe(res);
    return;
  }
  res.writeHead(200, { 'content-type': type, 'content-length': st.size, 'accept-ranges': 'bytes' });
  createReadStream(file).pipe(res);
};
createServer((req, res) => {
  try {
    let path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (shareDir && path.startsWith('/share/') && req.method !== 'GET') { upload(req, res, path); return; }
    const share = shareDir && /^\/share\/s\/([0-9A-Za-z]+)(?:\/f\/(\d+)\/(.+))?$/.exec(path);
    if (share) {
      const [, id, n, rest] = share;
      if (!existsSync(join(shareDir, id, 'share.json')) || existsSync(join(shareDir, id, 'pending'))) { res.writeHead(404); res.end(); return; }
      if (existsSync(join(shareDir, id, 'expired'))) { res.writeHead(410); res.end(); return; }
      serveFile(req, res, n === undefined ? join(shareDir, id, 'share.json') : join(shareDir, id, n, normalize(rest)));
      return;
    }
    if (path.endsWith('/')) path += 'index.html';
    serveFile(req, res, join(dir, normalize(path)));
  } catch {
    res.writeHead(404); res.end();
  }
}).listen(Number(port), '127.0.0.1');

const body = (req) => new Promise((resolve) => { const chunks = []; req.on('data', (c) => chunks.push(c)); req.on('end', () => resolve(Buffer.concat(chunks))); });
const LIFE = { '1h': 3600, '1d': 86400, '1w': 7 * 86400, '1m': 30 * 86400 };
async function upload(req, res, path) {
  if (req.headers.authorization !== 'Bearer test-token') { res.writeHead(401); res.end(); return; }
  const data = await body(req);
  const m = /^\/share\/s(?:\/([0-9A-Za-z]+)(?:\/(finish|art)|\/f\/(\d+)\/(.+))?)?$/.exec(path);
  if (!m) { res.writeHead(404); res.end(); return; }
  const [, id, verb, n, rest] = m;
  if (req.method === 'POST' && !id) {
    const want = JSON.parse(data.toString('utf8'));
    if (want.songs.length > 200) { res.writeHead(413); res.end(); return; }
    const newId = randomBytes(6).toString('base64url').replace(/[^0-9A-Za-z]/g, 'x').slice(0, 8).padEnd(8, 'x');
    const now = Math.floor(Date.now() / 1000);
    const base = `${req.headers.origin || 'http://127.0.0.1:' + port}/share`.replace(/^https?:\/\/localhost/, 'http://127.0.0.1');
    const share = {
      pack: want.pack, created_at: now, expires_at: want.expires in LIFE ? now + LIFE[want.expires] : null,
      songs: want.songs.map((s, i) => ({ record: s.record, files: s.files.map((f) => ({ path: f.path, bytes: f.bytes, url: `${base}/s/${newId}/f/${i}/${f.path.split('/').map(encodeURIComponent).join('/')}` })) })),
      pack_art: want.pack_art ? `${base}/s/${newId}/art` : null,
    };
    mkdirSync(join(shareDir, newId), { recursive: true });
    writeFileSync(join(shareDir, newId, 'share.json'), JSON.stringify(share));
    writeFileSync(join(shareDir, newId, 'pending'), '');
    res.writeHead(201, { 'content-type': 'application/json' }); res.end(JSON.stringify({ id: newId })); return;
  }
  if (!id || !existsSync(join(shareDir, id, 'share.json'))) { res.writeHead(404); res.end(); return; }
  if (req.method === 'DELETE') { rmSync(join(shareDir, id), { recursive: true, force: true }); res.writeHead(200); res.end(); return; }
  if (req.method === 'POST' && verb === 'finish') { unlinkSync(join(shareDir, id, 'pending')); res.writeHead(200); res.end(); return; }
  if (req.method === 'PUT' && (verb === 'art' || n !== undefined)) {
    if (data.length > 512 * 1024 * 1024) { res.writeHead(413); res.end(); return; }
    const file = verb === 'art' ? join(shareDir, id, 'art') : join(shareDir, id, n, normalize(rest));
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, data);
    res.writeHead(200); res.end(); return;
  }
  res.writeHead(405); res.end();
}
