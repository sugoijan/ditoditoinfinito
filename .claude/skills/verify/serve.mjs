// A static server for a test bundle: node serve.mjs <dir> <port> (127.0.0.1).
// `python3 -m http.server` resets connections under the page's parallel
// requests (Chrome then shows "This browser cannot run …"); this does not.
import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { join, extname, normalize } from 'node:path';
const [dir, port] = process.argv.slice(2);
const types = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.opus': 'audio/ogg', '.ogg': 'audio/ogg', '.lua': 'text/plain', '.ini': 'text/plain', '.txt': 'text/plain' };
createServer((req, res) => {
  try {
    let path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path.endsWith('/')) path += 'index.html';
    const file = join(dir, normalize(path));
    const st = statSync(file);
    if (!st.isFile()) throw new Error();
    res.writeHead(200, { 'content-type': types[extname(file).toLowerCase()] || 'application/octet-stream', 'content-length': st.size });
    createReadStream(file).pipe(res);
  } catch {
    res.writeHead(404); res.end();
  }
}).listen(Number(port), '127.0.0.1');
