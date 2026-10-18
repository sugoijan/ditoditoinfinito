// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Dito Dito Infinito contributors

import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import http from 'node:http';
import { EventEmitter } from 'node:events';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { gzipSync } from 'node:zlib';
import { CaptureNetwork } from '../network.mjs';

async function fixture(t, handler, robots = '') {
  const requests = [];
  const server = http.createServer((req, res) => {
    requests.push({ url: req.url, headers: req.headers, at: Date.now() });
    if (req.url === '/robots.txt') {
      res.writeHead(robots === null ? 404 : 200, { 'content-type': 'text/plain' });
      res.end(robots ?? 'not found');
    } else handler(req, res);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }));
  return { origin: `http://127.0.0.1:${server.address().port}`, requests };
}

async function network(t, origins, options = {}) {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'ddi-network-test-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return new CaptureNetwork({ cacheDir: path.join(directory, 'cache'), outDir: path.join(directory, 'work'),
    delayMs: 0, allowedOrigins: origins, ...options }).init();
}

test('deduplicates concurrent bodies, decodes transfer encoding, strips cookies, and retains policy evidence', async t => {
  const content = Buffer.from('chart data: 譜面\n');
  const { origin, requests } = await fixture(t, (_req, res) => {
    res.writeHead(200, { 'content-type': 'text/plain; charset=UTF-8', 'content-encoding': 'gzip',
      'set-cookie': 'secret=session', 'access-control-allow-origin': '*' });
    res.end(gzipSync(content));
  }, null);
  const capture = await network(t, [origin]);
  const results = await Promise.all(Array.from({ length: 15 }, () => capture.fetch(`${origin}/dos.txt`, { role: 'dos' })));
  assert.equal(requests.filter(item => item.url === '/dos.txt').length, 1);
  assert.equal(requests.filter(item => item.url === '/robots.txt').length, 1);
  assert.deepEqual(results[0].body, content);
  assert.equal(results[0].record.charset, 'UTF-8');
  assert.equal(results[0].record.headers['set-cookie'], undefined);
  assert.equal(results[0].record.headers['content-encoding'], undefined);
  assert.equal(requests[1].headers['user-agent'], 'DdiDanoniDump');
  assert.equal(requests[1].headers.cookie, undefined);
  assert.equal(requests[1].headers.range, undefined);
  const resources = await capture.save();
  assert.equal(resources.resources.length, 2);
  assert.equal(resources.resources.find(item => item.role === 'robots').status, 404);
  assert.deepEqual(await readFile(path.join(capture.outDir, results[0].record.bodyPath)), content);
});

test('enforces user-agent robots rules and crawl delay without fetching denied paths', async t => {
  const { origin, requests } = await fixture(t, (_req, res) => res.end('ok'),
    'User-agent: *\nDisallow: /\n\nUser-agent: DdiDanoniDump\nDisallow: /private\nCrawl-delay: 0.06\n');
  const capture = await network(t, [origin]);
  await assert.rejects(capture.fetch(`${origin}/private/song.mp3`), { code: 'robots-denied' });
  await Promise.all([capture.fetch(`${origin}/first`), capture.fetch(`${origin}/second`)]);
  assert.equal(requests.some(item => item.url.startsWith('/private')), false);
  for (let i = 1; i < requests.length; i++) assert.ok(requests[i].at - requests[i - 1].at >= 50);
});

test('robots failures fail closed and are not retried', async t => {
  const { origin, requests } = await fixture(t, (_req, res) => res.end('ok'));
  const capture = await network(t, [origin]);
  const original = capture._request.bind(capture);
  capture._request = async url => url.endsWith('/robots.txt')
    ? { status: 503, headers: {}, body: Buffer.from('unavailable') } : original(url);
  await assert.rejects(capture.fetch(`${origin}/song`), { code: 'robots-unavailable' });
  await assert.rejects(capture.fetch(`${origin}/song`), { code: 'robots-unavailable' });
  assert.equal(requests.length, 0);
  assert.equal(capture.records.filter(record => record.role === 'robots').length, 1);
});

test('rechecks robots for redirect origins and records the redirect chain', async t => {
  const target = await fixture(t, (_req, res) => res.end('final'), 'User-agent: *\nDisallow: /blocked\n');
  const source = await fixture(t, (req, res) => {
    res.writeHead(302, { location: `${target.origin}${req.url}` }); res.end('moved');
  });
  const capture = await network(t, [source.origin, target.origin]);
  await assert.rejects(capture.fetch(`${source.origin}/blocked`), { code: 'robots-denied' });
  assert.equal(target.requests.some(item => item.url === '/blocked'), false);
  const { record, body } = await capture.fetch(`${source.origin}/allowed`);
  assert.equal(body.toString(), 'final');
  assert.equal(record.finalUrl, `${target.origin}/allowed`);
  assert.equal(record.redirects.length, 1);
  assert.equal(record.redirects[0].status, 302);
  assert.equal(record.robots.length, 2);
  await capture.fetch(`${target.origin}/allowed`);
  assert.equal(target.requests.filter(item => item.url === '/allowed').length, 1);
});

test('rejects private addresses including redirect targets unless origin was explicitly allowed', async t => {
  const target = await fixture(t, (_req, res) => res.end('private'));
  const source = await fixture(t, (_req, res) => { res.writeHead(302, { location: `${target.origin}/secret` }); res.end(); });
  const capture = await network(t, [source.origin]);
  await assert.rejects(capture.fetch(`${target.origin}/secret`), { code: 'private-network' });
  await assert.rejects(capture.fetch(`${source.origin}/redirect`), { code: 'private-network' });
  assert.equal(target.requests.length, 0);
  await assert.rejects(capture.fetch('http://user:password@example.org/'), { code: 'unsupported-url' });
});

test('offline output replay needs no persistent cache or server and reports misses', async t => {
  const { origin, requests } = await fixture(t, (_req, res) => res.end('standalone'));
  const capture = await network(t, [origin]);
  await capture.fetch(`${origin}/song`);
  await capture.save();
  await rm(capture.cacheDir, { recursive: true, force: true });
  const replay = await new CaptureNetwork({ outDir: capture.outDir,
    cacheDir: capture.cacheDir, offline: true }).init();
  assert.equal((await replay.fetch(`${origin}/song`)).body.toString(), 'standalone');
  await assert.rejects(replay.fetch(`${origin}/uncaptured`), { code: 'offline-miss' });
  assert.deepEqual(replay.offlineMisses, [`${origin}/uncaptured`]);
  assert.equal(requests.length, 2);
});

test('preserves meaningful queries, persists explicit aliases, and rejects alias cycles', async t => {
  const { origin, requests } = await fixture(t, (req, res) => res.end(req.url));
  const capture = await network(t, [origin]);
  await capture.fetch(`${origin}/work?stageNum=1`);
  await capture.fetch(`${origin}/work?stageNum=2`);
  capture.addAlias(`${origin}/engine.js?12345`, `${origin}/engine.js`);
  assert.equal((await capture.fetch(`${origin}/engine.js?12345`)).body.toString(), '/engine.js');
  assert.equal((await capture.fetch(`${origin}/engine.js`)).body.toString(), '/engine.js');
  assert.equal(requests.filter(item => item.url.startsWith('/engine')).length, 1);
  assert.equal(requests.filter(item => item.url.startsWith('/work')).length, 2);
  assert.throws(() => capture.addAlias(`${origin}/engine.js`, `${origin}/engine.js?12345`), { code: 'alias-cycle' });
  assert.throws(() => capture.addAlias(`${origin}/engine.js`, 'https://example.org/engine.js'), { code: 'alias-origin' });
  await capture.save();
  const reopen = await new CaptureNetwork({ cacheDir: capture.cacheDir,
    outDir: path.join(capture.outDir, 'another'), offline: true }).init();
  assert.equal((await reopen.fetch(`${origin}/engine.js?12345`)).record.aliasOf, `${origin}/engine.js`);
});

test('detects a damaged cached body and refuses incomplete 206 responses', async t => {
  const { origin, requests } = await fixture(t, (req, res) => {
    if (req.url === '/partial') res.writeHead(206, { 'content-range': 'bytes 0-2/100' });
    res.end(req.url === '/partial' ? 'xyz' : 'abc');
  });
  const capture = await network(t, [origin]);
  const { record } = await capture.fetch(`${origin}/complete`);
  await writeFile(path.join(capture.cacheDir, record.bodyPath), 'bad');
  await assert.rejects(capture.fetch(`${origin}/complete`), { code: 'cache-integrity' });
  await assert.rejects(capture.fetch(`${origin}/partial`), { code: 'partial-response' });
  await assert.rejects(capture.fetch(`${origin}/partial`), { code: 'partial-response' });
  assert.equal(requests.filter(item => item.url === '/partial').length, 1);
});

test('browser routing preserves redirects, denies mutations, and handles closing contexts', async t => {
  const { origin } = await fixture(t, (req, res) => {
    if (req.url === '/start') { res.writeHead(302, { location: '/end' }); res.end(); }
    else res.end('end');
  });
  const capture = await network(t, [origin]);
  let handler;
  await capture.attach({ route: async (_pattern, callback) => { handler = callback; } });
  let fulfilled;
  let aborted = false;
  const route = { request: () => ({ url: () => `${origin}/start`, method: () => 'GET', resourceType: () => 'document' }),
    fulfill: async value => { fulfilled = value; }, abort: async () => { aborted = true; } };
  await handler(route);
  assert.equal(fulfilled.status, 302);
  assert.equal(new URL(fulfilled.headers.location, origin).href, `${origin}/end`);
  await handler({ ...route, request: () => ({ url: () => `${origin}/submit`, method: () => 'POST' }) });
  assert.equal(aborted, true);
  assert.ok(capture.diagnostics.some(item => item.code === 'method-denied'));
  await assert.doesNotReject(handler({ ...route,
    fulfill: async () => { throw new Error('Target page closed'); },
    abort: async () => { throw new Error('Target page closed'); } }));
});

test('a cached redirect exports a standalone dump, and offline verification can write elsewhere', async t => {
  const { origin, requests } = await fixture(t, (req, res) => {
    if (req.url === '/start') { res.writeHead(302, { location: '/end' }); res.end('moved'); }
    else res.end('complete');
  });
  const capture = await network(t, [origin]);
  await capture.fetch(`${origin}/start`);
  await capture.save();
  const secondOutput = path.join(capture.outDir, 'second');
  const reuse = await new CaptureNetwork({ cacheDir: capture.cacheDir, outDir: secondOutput,
    allowedOrigins: [origin], delayMs: 0 }).init();
  await reuse.fetch(`${origin}/start`);
  await reuse.save();
  assert.equal(reuse.records.some(item => item.requestedUrl === `${origin}/end`), true);
  const replay = await new CaptureNetwork({ cacheDir: secondOutput,
    outDir: path.join(capture.outDir, 'verification'), offline: true }).init();
  assert.equal((await replay.fetch(`${origin}/start`)).body.toString(), 'complete');
  assert.equal((await replay.fetch(`${origin}/end`)).body.toString(), 'complete');
  await replay.save();
  assert.equal(requests.length, 3);
});

test('warm-cache alias captures replay with no cache index and recover older alias-only dumps', async t => {
  const { origin, requests } = await fixture(t, (_req, res) => res.end('cached engine'));
  const warmup = await network(t, [origin]);
  const canonical = `${origin}/engine.js`;
  const alias = `${canonical}?1234567890123`;
  await warmup.fetch(canonical);
  await warmup.save();
  const second = await new CaptureNetwork({ cacheDir: warmup.cacheDir,
    outDir: path.join(warmup.outDir, 'warm-capture'), allowedOrigins: [origin], delayMs: 0 }).init();
  second.addAlias(alias, canonical);
  await second.fetch(alias);
  const resources = await second.save();
  assert.ok(resources.resources.some(record => record.requestedUrl === canonical));
  const replay = await new CaptureNetwork({ cacheDir: second.outDir,
    outDir: path.join(warmup.outDir, 'offline'), offline: true }).init();
  assert.equal((await replay.fetch(alias)).body.toString(), 'cached engine');
  assert.equal(replay.offlineMisses.length, 0);
  assert.equal(requests.filter(request => request.url === '/engine.js').length, 1);
  // Repair-compatible loading of dumps created before canonical records were
  // always exported; no origin request is permitted during this operation.
  resources.resources = resources.resources.filter(record => record.requestedUrl !== canonical);
  await writeFile(path.join(second.outDir, 'resources.json'), JSON.stringify(resources));
  const older = await new CaptureNetwork({ cacheDir: second.outDir,
    outDir: path.join(warmup.outDir, 'older-offline'), offline: true }).init();
  assert.equal((await older.fetch(alias)).body.toString(), 'cached engine');
  const conflict = { ...resources.resources.find(record => record.requestedUrl === alias),
    requestedUrl: canonical, sha256: '0'.repeat(64) };
  delete conflict.aliasOf;
  resources.resources.push(conflict);
  await writeFile(path.join(second.outDir, 'resources.json'), JSON.stringify(resources));
  await assert.rejects(new CaptureNetwork({ cacheDir: second.outDir,
    outDir: path.join(warmup.outDir, 'conflict-offline'), offline: true }).init(), { code: 'resource-conflict' });
});

async function browserRouter(capture) {
  let handler;
  await capture.attach({ route: async (_pattern, callback) => { handler = callback; } });
  return async (url, headers = {}) => {
    let result;
    await handler({ request: () => ({ url: () => url, method: () => 'GET', resourceType: () => 'media',
      allHeaders: async () => headers }), fulfill: async response => { result = response; },
      abort: async reason => { result = { aborted: reason }; } });
    return result;
  };
}

test('forwards only actual browser source headers and lets Chrome choose each redirect hop context', async t => {
  const { origin, requests } = await fixture(t, (req, res) => {
    if (req.url === '/redirect') {
      res.writeHead(302, { location: '/target', 'referrer-policy': 'no-referrer' }); res.end('redirect');
    } else res.end('audio');
  });
  const capture = await network(t, [origin]);
  const route = await browserRouter(capture);
  const sourceHeaders = { referer: `${origin}/work.html`, origin, accept: 'audio/*',
    cookie: 'private=value', authorization: 'Bearer secret', range: 'bytes=0-99', 'x-test': 'ignored' };
  const redirect = await route(`${origin}/redirect`, sourceHeaders);
  assert.equal(redirect.status, 302);
  assert.equal(redirect.headers['referrer-policy'], 'no-referrer');
  assert.equal(requests.some(request => request.url === '/target'), false);
  const sent = requests.find(request => request.url === '/redirect').headers;
  assert.equal(sent.referer, sourceHeaders.referer);
  assert.equal(sent.origin, origin);
  assert.equal(sent.accept, 'audio/*');
  for (const excluded of ['cookie', 'authorization', 'range', 'x-test']) assert.equal(sent[excluded], undefined);
  // Chrome supplies this second context after applying the response policy;
  // the capturer must not carry the first-hop Referer into it.
  await route(`${origin}/target`, { accept: 'audio/*' });
  assert.equal(requests.find(request => request.url === '/target').headers.referer, undefined);
  const captured = capture.records.find(record => record.requestedUrl === `${origin}/redirect`);
  assert.deepEqual(captured.requestContext, { kind: 'browser', headers: { accept: 'audio/*', origin,
    referer: `${origin}/work.html` } });
  assert.match(captured.requestContextFingerprint, /^[a-f0-9]{64}$/);
  await capture.fetch(`${origin}/explicit`);
  const explicit = requests.find(request => request.url === '/explicit').headers;
  assert.equal(explicit.referer, undefined);
  assert.equal(explicit.origin, undefined);
  assert.equal(explicit.accept, '*/*');
});

test('reuses anonymous cache absent Vary and refuses incompatible declared variants without refetching', async t => {
  const { origin, requests } = await fixture(t, (req, res) => {
    if (req.url === '/varied') res.setHeader('Vary', 'Origin, Accept');
    if (req.url === '/star') res.setHeader('Vary', '*');
    if (req.url === '/unknown') res.setHeader('Vary', 'X-Unknown');
    if (req.url === '/protected') res.writeHead(403);
    res.end('response');
  });
  const capture = await network(t, [origin]);
  for (const url of ['/plain', '/varied', '/protected']) await capture.fetch(`${origin}${url}`);
  const route = await browserRouter(capture);
  const headers = { referer: `${origin}/work`, origin, accept: 'audio/*' };
  assert.equal((await route(`${origin}/plain`, headers)).status, 200);
  assert.equal((await route(`${origin}/protected`, headers)).status, 403);
  assert.equal((await route(`${origin}/varied`, headers)).aborted, 'blockedbyclient');
  assert.ok(capture.diagnostics.some(item => item.code === 'cache-vary-mismatch'));
  for (const url of ['/plain', '/varied', '/protected']) {
    assert.equal(requests.filter(request => request.url === url).length, 1);
  }
  assert.equal((await route(`${origin}/star`, headers)).status, 200);
  assert.equal((await route(`${origin}/star`, headers)).aborted, 'blockedbyclient');
  assert.equal((await route(`${origin}/unknown`, headers)).status, 200);
  assert.equal((await route(`${origin}/unknown`, headers)).aborted, 'blockedbyclient');
  const plain = capture.records.find(record => record.requestedUrl === `${origin}/plain`);
  assert.equal(plain.requestContext.kind, 'anonymous');
  assert.ok(plain.observedRequestContexts.some(context => context.kind === 'browser'));
});

test('matching declared browser variants replay offline and recorded redirect hops remain navigable', async t => {
  const { origin, requests } = await fixture(t, (req, res) => {
    res.setHeader('Vary', 'Origin, Referer');
    if (req.url === '/start') res.writeHead(302, { location: '/end' });
    res.end(req.url);
  });
  const capture = await network(t, [origin]);
  const route = await browserRouter(capture);
  const headers = { origin, referer: `${origin}/work` };
  await route(`${origin}/start`, headers);
  await route(`${origin}/end`, headers);
  await capture.save();
  const offline = await new CaptureNetwork({ cacheDir: capture.outDir,
    outDir: path.join(capture.outDir, 'verify'), offline: true }).init();
  const replay = await browserRouter(offline);
  assert.equal((await replay(`${origin}/start`, headers)).status, 302);
  assert.equal((await replay(`${origin}/end`, headers)).status, 200);
  assert.equal(offline.offlineMisses.length, 0);
  assert.equal(requests.length, 3);
});

test('total deadlines stop trickling bodies and persist failed URLs across captures', async t => {
  const { origin, requests } = await fixture(t, (_req, res) => {
    const timer = setInterval(() => res.write('still streaming'), 10);
    res.on('close', () => clearInterval(timer));
  });
  const capture = await network(t, [origin], { requestTimeoutMs: 80 });
  const start = Date.now();
  await assert.rejects(capture.fetch(`${origin}/never-finishes`), { code: 'request-timeout' });
  assert.ok(Date.now() - start < 1500);
  await assert.rejects(capture.fetch(`${origin}/never-finishes`), { code: 'request-timeout' });
  await capture.save();
  const later = await new CaptureNetwork({ cacheDir: capture.cacheDir,
    outDir: path.join(capture.outDir, 'later'), delayMs: 0, allowedOrigins: [origin] }).init();
  await assert.rejects(later.fetch(`${origin}/never-finishes`), { code: 'request-timeout' });
  assert.equal(requests.filter(request => request.url === '/never-finishes').length, 1);
});

test('context closure cancels active and pending requests without fetching unstarted queue entries', async t => {
  let markStarted;
  const started = new Promise(resolve => { markStarted = resolve; });
  const { origin, requests } = await fixture(t, (req, res) => {
    if (req.url === '/hang') markStarted();
    else res.end('available');
  });
  const capture = await network(t, [origin]);
  const context = new EventEmitter();
  context.route = async () => {};
  await capture.attach(context);
  const pending = assert.rejects(capture.fetch(`${origin}/hang`), { code: 'request-cancelled' });
  await started;
  capture.delayMs = 10_000;
  const queued = assert.rejects(capture.fetch(`${origin}/queued`), { code: 'capture-cancelled' });
  const closing = Date.now();
  context.emit('close');
  await Promise.all([pending, queued]);
  assert.ok(Date.now() - closing < 1500);
  assert.equal(requests.some(request => request.url === '/queued'), false);
  capture.delayMs = 0;
  assert.equal((await capture.fetch(`${origin}/queued`)).body.toString(), 'available');
  assert.equal(requests.filter(request => request.url === '/queued').length, 1);
});

test('never-started cancellation is not persisted and a later warm-cache capture can fetch that URL', async t => {
  const { origin, requests } = await fixture(t, (_req, res) => res.end('available'));
  const capture = await network(t, [origin]);
  await capture.fetch(`${origin}/baseline`);
  capture.delayMs = 10_000;
  const cancelled = assert.rejects(capture.fetch(`${origin}/later`), { code: 'capture-cancelled' });
  capture.cancelPending('Unsupported engine stopped this chart before queued assets were requested');
  await cancelled;
  const resources = await capture.save();
  assert.equal(requests.some(request => request.url === '/later'), false);
  assert.equal(resources.failures[`${origin}/later`], undefined);
  const index = JSON.parse(await readFile(path.join(capture.cacheDir, 'index.json'), 'utf8'));
  assert.equal(index.failures[`${origin}/later`], undefined);
  const newerAdapter = await new CaptureNetwork({ cacheDir: capture.cacheDir,
    outDir: path.join(capture.outDir, 'later-capture'), delayMs: 0, allowedOrigins: [origin] }).init();
  assert.equal((await newerAdapter.fetch(`${origin}/later`)).body.toString(), 'available');
  assert.equal(requests.filter(request => request.url === '/later').length, 1);
});

test('known robots refusals replay offline without new requests or offline misses', async t => {
  const { origin, requests } = await fixture(t, (_req, res) => res.end('engine fallback'),
    'User-agent: *\nDisallow: /blocked-cdn\n');
  const capture = await network(t, [origin]);
  const blocked = `${origin}/blocked-cdn/engine.js`;
  await assert.rejects(capture.fetch(blocked), { code: 'robots-denied' });
  await capture.fetch(`${origin}/fallback.js`);
  const resources = await capture.save();
  assert.equal(resources.failures[blocked].code, 'robots-denied');
  const replay = await new CaptureNetwork({ cacheDir: capture.outDir,
    outDir: path.join(capture.outDir, 'offline'), offline: true }).init();
  await assert.rejects(replay.fetch(blocked), { code: 'robots-denied' });
  assert.equal((await replay.fetch(`${origin}/fallback.js`)).body.toString(), 'engine fallback');
  assert.deepEqual(replay.offlineMisses, []);
  assert.ok(replay.diagnostics.some(item => item.code === 'robots-denied'));
  assert.equal(requests.some(request => request.url.startsWith('/blocked-cdn')), false);
  delete resources.failures;
  await writeFile(path.join(capture.outDir, 'resources.json'), JSON.stringify(resources));
  const olderDump = await new CaptureNetwork({ cacheDir: capture.outDir,
    outDir: path.join(capture.outDir, 'older-offline'), offline: true }).init();
  await assert.rejects(olderDump.fetch(blocked), { code: 'robots-denied' });
  assert.deepEqual(olderDump.offlineMisses, []);
});

test('reviewed timestamp aliases migrate old warm-cache and resources-only responses without refetching', async t => {
  const { origin, requests } = await fixture(t, (_req, res) => res.end('51.2.2'));
  const oldProbe = `${origin}/version.txt?1700000000000`;
  const currentProbe = `${origin}/version.txt?1800000000000`;
  const canonical = `${origin}/version.txt`;
  const oldCapture = await network(t, [origin]);
  await oldCapture.fetch(oldProbe);
  await oldCapture.save();
  const warm = await new CaptureNetwork({ cacheDir: oldCapture.cacheDir,
    outDir: path.join(oldCapture.outDir, 'warm-migration'), delayMs: 0, allowedOrigins: [origin] }).init();
  warm.addAlias(currentProbe, canonical);
  assert.equal((await warm.fetch(currentProbe)).body.toString(), '51.2.2');
  const resources = await warm.save();
  assert.equal(requests.filter(request => request.url.startsWith('/version.txt')).length, 1);
  assert.equal(resources.resources.find(record => record.requestedUrl === canonical).cacheMigration.sourceUrl, oldProbe);
  assert.ok(resources.resources.some(record => record.requestedUrl === oldProbe));
  // The old output has neither an index nor alias metadata, only the original
  // timestamp response. Reviewed registration can still reuse it offline.
  const offline = await new CaptureNetwork({ cacheDir: oldCapture.outDir,
    outDir: path.join(oldCapture.outDir, 'offline-migration'), offline: true }).init();
  offline.addAlias(currentProbe, canonical);
  assert.equal((await offline.fetch(currentProbe)).body.toString(), '51.2.2');
  assert.deepEqual(offline.offlineMisses, []);
  assert.equal(requests.filter(request => request.url.startsWith('/version.txt')).length, 1);
});

test('reviewed timestamp migration rejects conflicting bodies and ignores semantic queries', async t => {
  const { origin, requests } = await fixture(t, (req, res) => res.end(req.url));
  const capture = await network(t, [origin]);
  await capture.fetch(`${origin}/version.txt?1700000000000`);
  await capture.fetch(`${origin}/version.txt?1700000000001`);
  assert.throws(() => capture.addAlias(`${origin}/version.txt?1800000000000`, `${origin}/version.txt`),
    { code: 'alias-cache-conflict' });
  assert.equal(capture.aliases.has(`${origin}/version.txt?1800000000000`), false);
  assert.equal(requests.filter(request => request.url.startsWith('/version.txt')).length, 2);
  await capture.fetch(`${origin}/other.txt?stageNum=1`);
  capture.addAlias(`${origin}/other.txt?1800000000000`, `${origin}/other.txt`);
  assert.equal(capture.entries.has(`${origin}/other.txt`), false);
});

test('reviewed timestamp migration also preserves failed attempts instead of retrying a new alias', async t => {
  const { origin, requests } = await fixture(t, (req) => req.socket.destroy());
  const capture = await network(t, [origin]);
  const oldProbe = `${origin}/version.txt?1700000000000`;
  await assert.rejects(capture.fetch(oldProbe), { code: 'ECONNRESET' });
  await capture.save();
  const newer = await new CaptureNetwork({ cacheDir: capture.cacheDir,
    outDir: path.join(capture.outDir, 'newer'), delayMs: 0, allowedOrigins: [origin] }).init();
  const newProbe = `${origin}/version.txt?1800000000000`;
  newer.addAlias(newProbe, `${origin}/version.txt`);
  await assert.rejects(newer.fetch(newProbe), { code: 'ECONNRESET' });
  assert.equal(requests.filter(request => request.url.startsWith('/version.txt')).length, 1);
});
