// A static server for a test bundle: node serve.mjs <dir> <port> [--share <sharedir>] (127.0.0.1).
// `python3 -m http.server` resets connections under the page's parallel
// requests (Chrome then shows "This browser cannot run …"); this does not.
// With --share, it stands in for the shared-songs Worker under /share
// (`docs/plans/shared-songs.md`): GET /share/s/<id> answers
// <sharedir>/<id>/share.json (404 without it, 410 with a file `expired`
// beside it), GET /share/s/<id>/f/<n>/<path> a file at <sharedir>/<id>/<n>/<path>
// with range requests; the page sets `window.__DDI_SHARE_URL` to it.
import { createServer } from 'node:http';
import { createReadStream, statSync, existsSync } from 'node:fs';
import { join, extname, normalize } from 'node:path';
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
    const share = shareDir && /^\/share\/s\/([0-9A-Za-z]+)(?:\/f\/(\d+)\/(.+))?$/.exec(path);
    if (share) {
      const [, id, n, rest] = share;
      if (!existsSync(join(shareDir, id, 'share.json'))) { res.writeHead(404); res.end(); return; }
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
