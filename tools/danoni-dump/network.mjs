// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Dito Dito Infinito contributors

import { createHash, randomUUID } from 'node:crypto';
import { lookup } from 'node:dns/promises';
import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import http from 'node:http';
import https from 'node:https';
import { isIP } from 'node:net';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { createBrotliDecompress, createGunzip, createInflate } from 'node:zlib';
import robotsParser from 'robots-parser';

const USER_AGENT = 'DdiDanoniDump';
const MAX_BYTES = 128 * 1024 * 1024;
const MAX_REDIRECTS = 8;
const CACHE_SCHEMA = 'ddi-danoni-cache/1';
const RESOURCE_SCHEMA = 'ddi-danoni-resources/1';
const KEPT_HEADERS = new Set([
  'content-type', 'cache-control', 'etag', 'last-modified', 'location',
  'access-control-allow-origin', 'access-control-allow-methods',
  'access-control-allow-headers', 'access-control-expose-headers',
  'cross-origin-resource-policy', 'cross-origin-opener-policy',
  'cross-origin-embedder-policy', 'content-security-policy',
  'content-security-policy-report-only', 'content-range', 'accept-ranges',
  'vary', 'date', 'expires', 'x-content-type-options', 'content-language', 'referrer-policy',
]);

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const failure = (code, message) => Object.assign(new Error(message), { code });

function abortable(promise, signal) {
  if (signal.aborted) return Promise.reject(signal.reason);
  return new Promise((resolve, reject) => {
    const abort = () => reject(signal.reason);
    signal.addEventListener('abort', abort, { once: true });
    promise.then(resolve, reject).finally(() => signal.removeEventListener('abort', abort));
  });
}


function normalizeUrl(value) {
  const url = new URL(value);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) {
    throw failure('unsupported-url', `Only anonymous HTTP(S) URLs are supported: ${value}`);
  }
  url.hash = '';
  return url.href;
}

function privateAddress(address) {
  if (isIP(address) === 4) {
    const [a, b, c] = address.split('.').map(Number);
    return a === 0 || a === 10 || a === 127 || a >= 224
      || (a === 100 && b >= 64 && b <= 127)
      || (a === 169 && b === 254) || (a === 172 && b >= 16 && b <= 31)
      || (a === 192 && b === 168) || (a === 192 && b === 0 && c <= 2)
      || (a === 198 && (b === 18 || b === 19 || (b === 51 && c === 100)))
      || (a === 203 && b === 0 && c === 113);
  }
  if (isIP(address) === 6) {
    // Only ordinary global unicast; excludes loopback, ULA, link-local,
    // multicast, IPv4-mapped addresses and translation mechanisms.
    const first = parseInt(address.split(':')[0], 16);
    return !(first >= 0x2000 && first <= 0x3fff)
      || /^2001:0?db8:/i.test(address) || /^2001:0{0,4}:/i.test(address);
  }
  return true;
}

async function atomicJson(filename, value) {
  await mkdir(path.dirname(filename), { recursive: true });
  const temporary = `${filename}.${randomUUID()}.tmp`;
  await writeFile(temporary, `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' });
  await rename(temporary, filename);
}

async function readJson(filename) {
  try { return JSON.parse(await readFile(filename, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return undefined; throw error; }
}

function filteredHeaders(headers) {
  return Object.fromEntries(Object.entries(headers)
    .filter(([key, value]) => KEPT_HEADERS.has(key.toLowerCase()) && value !== undefined)
    .map(([key, value]) => [key.toLowerCase(), Array.isArray(value) ? value.join(', ') : String(value)]));
}

const ANONYMOUS_CONTEXT = { kind: 'anonymous', headers: { accept: '*/*' } };

function browserContext(headers) {
  const selected = {};
  for (const name of ['accept', 'origin', 'referer']) {
    if (typeof headers[name] === 'string') selected[name] = headers[name];
  }
  return { kind: 'browser', headers: selected };
}

function contextFingerprint(context) {
  return digest(Buffer.from(JSON.stringify(Object.entries(context.headers).sort())));
}

function assertVariant(record, context, reusing = true) {
  const hop = record.redirects?.find(value => value.url === record.requestedUrl);
  const vary = ((hop?.headers ?? record.headers).vary ?? '').toLowerCase().split(',').map(value => value.trim()).filter(Boolean);
  const original = hop?.requestContext ?? record.requestContext ?? ANONYMOUS_CONTEXT;
  const fixed = { 'user-agent': USER_AGENT, 'accept-encoding': 'gzip, deflate, br',
    cookie: '', authorization: '', range: '', host: new URL(record.requestedUrl).host };
  for (const header of vary) {
    if (header === '*') {
      if (reusing) throw failure('cache-vary-unsupported', `Cannot reuse Vary: * response: ${record.requestedUrl}`);
      continue;
    }
    if (!['accept', 'origin', 'referer'].includes(header) && !(header in fixed)) {
      if (reusing) throw failure('cache-vary-unsupported', `Cannot verify Vary: ${header} cache context: ${record.requestedUrl}`);
      continue;
    }
    const previous = original.headers[header] ?? fixed[header] ?? '';
    const current = context.headers[header] ?? fixed[header] ?? '';
    if (previous !== current) {
      throw failure('cache-vary-mismatch', `Vary: ${header} differs from captured request; no refetch: ${record.requestedUrl}`);
    }
  }
}

/** Anonymous, robots-aware GET capture with replay under original URLs.
 * allowedOrigins only exempts explicitly named origins from private-address
 * rejection (for synthetic loopback fixtures); it is not a public-host allowlist.
 * One cache directory must have a single writer at a time.
 */
export class CaptureNetwork {
  constructor({ cacheDir, outDir, offline = false, delayMs = 1000, allowedOrigins = [], requestTimeoutMs = 30_000 }) {
    if (!outDir) throw new TypeError('outDir is required');
    if (!Number.isFinite(delayMs) || delayMs < 0) throw new TypeError('delayMs must be nonnegative');
    if (!Number.isFinite(requestTimeoutMs) || requestTimeoutMs <= 0) throw new TypeError('requestTimeoutMs must be positive');
    this.requestTimeoutMs = requestTimeoutMs;
    this.outDir = path.resolve(outDir);
    this.cacheDir = path.resolve(cacheDir ?? outDir);
    this.offline = offline;
    this.delayMs = delayMs;
    this.allowedOrigins = new Set(allowedOrigins.map(value => new URL(normalizeUrl(value)).origin));
    this.records = [];
    this.diagnostics = [];
    this.offlineMisses = [];
    this.aliases = new Map();
    this.entries = new Map();
    this.inflight = new Map();
    this.responses = new Map();
    this.failures = new Map();
    this.recordedFailures = new Map();
    this.lifecycle = new AbortController();
    this.contexts = new Set();
    this.policies = new Map();
    this.queues = new Map();
    this.lastStarts = new Map();
    this.crawlDelays = new Map();
    this.recordByUrl = new Map();
    this.indexWrite = Promise.resolve();
  }

  async init() {
    await Promise.all([this.cacheDir, this.outDir].map(dir => mkdir(path.join(dir, 'blobs/sha256'), { recursive: true })));
    const index = await readJson(path.join(this.cacheDir, 'index.json'));
    if (index) {
      if (index.schema !== CACHE_SCHEMA) throw failure('cache-schema', 'Unsupported cache index schema');
      this.entries = new Map(Object.entries(index.entries ?? {}));
      this.failures = new Map(Object.entries(index.failures ?? {}));
      this.aliases = new Map(index.aliases ?? []);
    }
    // A complete output directory is also sufficient for offline verification.
    if (this.offline) {
      for (const [root, storage] of [[this.cacheDir, 'cache'], [this.outDir, 'output']]) {
        const resources = await readJson(path.join(root, 'resources.json'));
        if (!resources) continue;
        if (resources.schema !== RESOURCE_SCHEMA) throw failure('resource-schema', 'Unsupported resource schema');
        const imported = new Map();
        for (const record of resources.resources) {
          const previous = imported.get(record.requestedUrl);
          if (previous && previous.sha256 !== record.sha256) {
            throw failure('resource-conflict', `Conflicting captured bodies for ${record.requestedUrl}`);
          }
          imported.set(record.requestedUrl, { ...record, storage });
        }
        // Older dumps may contain only an alias record when the canonical body
        // came from a warm cache. Its hash identifies the same captured body.
        for (const record of resources.resources) {
          if (!record.aliasOf) continue;
          const canonicalUrl = normalizeUrl(record.aliasOf);
          if (new URL(record.requestedUrl).origin !== new URL(canonicalUrl).origin) {
            throw failure('alias-origin', 'Captured alias crosses origins');
          }
          const canonical = imported.get(canonicalUrl);
          if (canonical && (canonical.sha256 !== record.sha256 || canonical.byteLength !== record.byteLength)) {
            throw failure('resource-conflict', `Alias body conflicts with canonical resource ${canonicalUrl}`);
          }
          if (!canonical) {
            const restored = { ...record, requestedUrl: canonicalUrl, url: canonicalUrl, storage };
            delete restored.aliasOf;
            imported.set(canonicalUrl, restored);
          }
        }
        for (const [url, record] of imported) this.entries.set(url, record);
        for (const [url, failed] of Object.entries(resources.failures ?? {})) this.failures.set(url, failed);
        // Earlier outputs recorded known access refusals only as diagnostics.
        // Do not infer failures from cancellation or generic capture errors.
        if (!resources.failures) {
          for (const diagnostic of resources.diagnostics ?? []) {
            if (['robots-denied', 'robots-unavailable', 'private-network'].includes(diagnostic.code)
                && typeof diagnostic.url === 'string' && /^https?:/.test(diagnostic.url)) {
              this.failures.set(normalizeUrl(diagnostic.url), { code: diagnostic.code, message: diagnostic.message,
                failedAt: null, recoveredFrom: 'diagnostic' });
            }
          }
        }
        for (const pair of resources.aliases ?? []) this.aliases.set(...pair);
        for (const [alias, canonical] of this.aliases) {
          if (this.failures.has(alias) && !this.failures.has(canonical)) this.failures.set(canonical, this.failures.get(alias));
        }
      }
    }
    return this;
  }

  /** Register an adapter-proven cache alias; query parameters are never guessed. */
  addAlias(requestUrl, canonicalUrl) {
    const request = normalizeUrl(requestUrl);
    const canonical = normalizeUrl(canonicalUrl);
    if (request === canonical) return;
    if (new URL(request).origin !== new URL(canonical).origin) {
      throw failure('alias-origin', 'Cache aliases must remain on the same origin');
    }
    let next = canonical;
    const seen = new Set([request]);
    while (this.aliases.has(next)) {
      if (seen.has(next)) throw failure('alias-cycle', 'Cache alias cycle');
      seen.add(next);
      next = this.aliases.get(next);
    }
    if (seen.has(next)) throw failure('alias-cycle', 'Cache alias cycle');
    const previous = this.aliases.get(request);
    if (previous && previous !== canonical) throw failure('alias-conflict', `Conflicting alias for ${request}`);
    this._migrateReviewedTimestampAlias(request, canonical);
    this.aliases.set(request, canonical);
    this.diagnostics.push({ code: 'cache-alias', requestedUrl: request, canonicalUrl: canonical,
      message: `Adapter registered cache alias ${request} -> ${canonical}` });
  }

  _migrateReviewedTimestampAlias(request, canonical) {
    if (this.entries.has(canonical) || this.failures.has(canonical)) return;
    const requestedUrl = new URL(request);
    const canonicalUrl = new URL(canonical);
    // This is invoked only by an explicit adapter-reviewed alias. It does not
    // discover aliases itself or remove semantic/mixed query parameters.
    if (!/^\?\d{13}$/.test(requestedUrl.search) || canonicalUrl.search
        || requestedUrl.origin !== canonicalUrl.origin || requestedUrl.pathname !== canonicalUrl.pathname) return;
    const candidates = [...this.entries].filter(([key]) => {
      const url = new URL(key);
      return url.origin === canonicalUrl.origin && url.pathname === canonicalUrl.pathname && /^\?\d{13}$/.test(url.search);
    });
    if (!candidates.length) {
      const priorFailures = [...this.failures].filter(([key]) => {
        const url = new URL(key);
        return url.origin === canonicalUrl.origin && url.pathname === canonicalUrl.pathname && /^\?\d{13}$/.test(url.search);
      }).sort(([, first], [, second]) => String(second.failedAt).localeCompare(String(first.failedAt)));
      if (priorFailures.length) {
        const [sourceUrl, failed] = priorFailures[0];
        this.failures.set(canonical, { ...failed, cacheMigration: { sourceUrl, reviewedRequestUrl: request, canonicalUrl: canonical } });
        this.diagnostics.push({ code: 'cache-alias-failure-migrated', url: canonical, sourceUrl,
          message: `Preserved failed timestamp request for reviewed alias without retry: ${sourceUrl}` });
      }
      return;
    }
    const identities = new Set(candidates.map(([, record]) => JSON.stringify([
      record.sha256, record.byteLength, record.status, record.headers['content-type'] ?? '',
      record.headers.vary ?? '', record.headers.location ?? '',
    ])));
    if (identities.size !== 1) {
      throw failure('alias-cache-conflict', `Reviewed timestamp alias has conflicting cached responses; no refetch: ${canonical}`);
    }
    // Prefer the exact request, otherwise the newest captured response. Keep
    // its real final URL, request headers, storage location and retrieval time.
    candidates.sort(([a, first], [b, second]) =>
      Number(b === request) - Number(a === request) || String(second.retrievedAt).localeCompare(String(first.retrievedAt)));
    const [sourceUrl, source] = candidates[0];
    const restored = { ...source, requestedUrl: canonical, url: canonical,
      cacheMigration: { sourceUrl, reviewedRequestUrl: request, canonicalUrl: canonical } };
    delete restored.aliasOf;
    this.entries.set(canonical, restored);
    this.diagnostics.push({ code: 'cache-alias-migrated', url: canonical, sourceUrl,
      message: `Reused previously captured timestamp response for reviewed alias: ${sourceUrl} -> ${canonical}` });
  }

  async fetch(value, { role = 'other' } = {}) {
    return this._fetchWithContext(value, { role }, ANONYMOUS_CONTEXT);
  }

  async _fetchWithContext(value, { role = 'other', followRedirects = true } = {}, requestContext) {
    const requestedUrl = normalizeUrl(value);
    let canonicalUrl = requestedUrl;
    const seen = new Set();
    while (this.aliases.has(canonicalUrl)) {
      if (seen.has(canonicalUrl)) throw failure('alias-cycle', 'Cache alias cycle');
      seen.add(canonicalUrl);
      canonicalUrl = this.aliases.get(canonicalUrl);
    }
    let pending = this.inflight.get(canonicalUrl);
    const reusedInflight = Boolean(pending);
    if (!pending) {
      pending = this._fetchCanonical(canonicalUrl, role, { followRedirects, requestContext });
      this.inflight.set(canonicalUrl, pending);
      pending.catch(() => {}).finally(() => {
        if (this.inflight.get(canonicalUrl) === pending) this.inflight.delete(canonicalUrl);
      });
    }
    try {
      let { record: canonical, body } = await pending;
      assertVariant(canonical, requestContext, reusedInflight || canonical.fromCache);
      if (followRedirects && [301, 302, 303, 307, 308].includes(canonical.status) && canonical.headers.location) {
        ({ record: canonical, body } = await this._followRedirect(canonicalUrl, canonical, body, role,
          { robots: false, redirects: [], signal: this.lifecycle.signal, requestContext }));
      }
      if (canonicalUrl !== requestedUrl) await this._record(canonical, body);
      const record = { ...canonical, requestedUrl, url: requestedUrl, role, observedRequestContext: requestContext };
      if (canonicalUrl !== requestedUrl) record.aliasOf = canonicalUrl;
      await this._record(record, body);
      delete record.storage;
      return { record, body };
    } catch (error) {
      if (!this.diagnostics.some(item => item.url === requestedUrl && item.code === (error.code ?? 'fetch-error'))) {
        this.diagnostics.push({ code: error.code ?? 'fetch-error', url: requestedUrl, message: error.message });
      }
      // A known refusal/failure is replayable evidence. A cancelled, unstarted
      // queue slot has no response and may be needed by a later chart/adapter.
      if (!['capture-cancelled', 'offline-miss'].includes(error.code)) {
        const failed = { code: error.code ?? 'fetch-error', message: error.message, failedAt: new Date().toISOString() };
        this.recordedFailures.set(canonicalUrl, failed);
        this.recordedFailures.set(requestedUrl, failed);
      }
      throw error;
    }
  }

  async _readBody(record) {
    if (!/^[a-f0-9]{64}$/.test(record.sha256)) throw failure('cache-integrity', 'Invalid cached body hash');
    const root = record.storage === 'output' ? this.outDir : this.cacheDir;
    const body = await readFile(path.join(root, 'blobs/sha256', record.sha256));
    if (body.length !== record.byteLength || digest(body) !== record.sha256) {
      throw failure('cache-integrity', `Cached body verification failed for ${record.requestedUrl}`);
    }
    return body;
  }

  async _record(record, body) {
    const filename = path.join(this.outDir, 'blobs/sha256', record.sha256);
    // Concurrent equivalent bodies can share the immutable content-addressed file.
    await writeFile(filename, body, { flag: 'wx' }).catch(error => { if (error.code !== 'EEXIST') throw error; });
    const previous = this.recordByUrl.get(record.requestedUrl);
    if (previous) {
      if (!previous.roles.includes(record.role)) previous.roles.push(record.role);
      const context = record.observedRequestContext ?? record.requestContext ?? ANONYMOUS_CONTEXT;
      if (!previous.observedRequestContexts.some(value => JSON.stringify(value) === JSON.stringify(context))) {
        previous.observedRequestContexts.push(context);
      }
      return;
    }
    const entry = { ...record, roles: [record.role],
      observedRequestContexts: [record.observedRequestContext ?? record.requestContext ?? ANONYMOUS_CONTEXT] };
    delete entry.observedRequestContext;
    delete entry.storage;
    this.recordByUrl.set(record.requestedUrl, entry);
    this.records.push(entry);
    const migratedSource = this.entries.get(record.cacheMigration?.sourceUrl);
    if (migratedSource && !this.recordByUrl.has(migratedSource.requestedUrl)) {
      await this._record(migratedSource, await this._readBody(migratedSource));
    }
    for (const redirect of record.redirects ?? []) {
      const redirectBody = await this._readBody({ ...redirect, storage: record.storage });
      await writeFile(path.join(this.outDir, 'blobs/sha256', redirect.sha256), redirectBody, { flag: 'wx' })
        .catch(error => { if (error.code !== 'EEXIST') throw error; });
      const target = this.entries.get(redirect.location);
      if (target && !this.recordByUrl.has(target.requestedUrl)) await this._record(target, await this._readBody(target));
    }
  }

  async _fetchCanonical(url, role, { robots = false, redirects = [], signal = this.lifecycle.signal,
    followRedirects = true, requestContext = ANONYMOUS_CONTEXT } = {}) {
    const cached = this.entries.get(url);
    if (cached) {
      assertVariant(cached, requestContext);
      // Include the stored policy response in every new work's provenance.
      for (const policy of cached.robots ?? []) {
        const record = this.entries.get(policy.url);
        if (record) await this._record(record, await this._readBody(record));
      }
      const body = await this._readBody(cached);
      if (cached.status === 206) {
        await this._record(cached, body);
        throw failure('partial-response', `Cached response is incomplete: ${url}`);
      }
      if (followRedirects && [301, 302, 303, 307, 308].includes(cached.status) && cached.headers.location) {
        return this._followRedirect(url, cached, body, role, { robots, redirects, signal, requestContext });
      }
      return { record: { ...cached, fromCache: true }, body };
    }
    const failed = this.failures.get(url);
    if (failed) {
      const policy = this.entries.get(`${new URL(url).origin}/robots.txt`);
      if (policy) await this._record(policy, await this._readBody(policy));
      throw failure(failed.code, failed.message);
    }
    if (this.offline) {
      if (!this.offlineMisses.includes(url)) this.offlineMisses.push(url);
      throw failure('offline-miss', `Offline cache miss: ${url}`);
    }
    const policies = robots ? [] : [await this._checkRobots(url, signal)];
    const response = await this._queuedRequest(url, signal, requestContext);
    let { status, headers, body } = response;
    if (followRedirects && [301, 302, 303, 307, 308].includes(status) && headers.location) {
      return this._followRedirect(url, response, body, role, { robots, redirects, signal, requestContext });
    }
    const contentType = headers['content-type'] ?? 'application/octet-stream';
    const record = { requestedUrl: url, url, finalUrl: url, status, headers,
      mimeType: contentType.split(';')[0].trim().toLowerCase(),
      charset: contentType.match(/charset\s*=\s*["']?([^;"'\s]+)/i)?.[1] ?? null,
      sha256: digest(body), bodyPath: `blobs/sha256/${digest(body)}`,
      byteLength: body.length, completeBody: status !== 206, retrievedAt: new Date().toISOString(),
      redirects: [], robots: policies, role, fromCache: false,
      requestContext: response.requestContext ?? requestContext,
      requestContextFingerprint: contextFingerprint(response.requestContext ?? requestContext) };
    await this._cache(record, body);
    await this._record(record, body);
    if (status === 206) throw failure('partial-response', `Server supplied an incomplete response without Range: ${url}`);
    return { record, body };
  }

  async _followRedirect(url, response, body, role, { robots, redirects, signal, requestContext }) {
    const { status, headers } = response;
    const policies = robots ? [] : [await this._checkRobots(url, signal)];
    if (redirects.length >= MAX_REDIRECTS) throw failure('redirect-limit', `Too many redirects: ${url}`);
    const target = normalizeUrl(new URL(headers.location, url).href);
    if (redirects.some(item => item.url === target) || target === url) {
      throw failure('redirect-loop', `Redirect loop: ${url}`);
    }
    const redirectHash = digest(body);
    const hop = { url, status, location: target, headers, sha256: redirectHash,
      bodyPath: `blobs/sha256/${redirectHash}`, byteLength: body.length, retrievedAt: new Date().toISOString(),
      requestContext: response.requestContext ?? requestContext };
    for (const root of [this.cacheDir, this.outDir]) {
      await writeFile(path.join(root, 'blobs/sha256', redirectHash), body, { flag: 'wx' })
        .catch(error => { if (error.code !== 'EEXIST') throw error; });
    }
    const redirected = await this._fetchCanonical(target, role,
      { robots, redirects: [...redirects, hop], signal, requestContext });
    const record = { ...redirected.record, requestedUrl: url, url,
      redirects: [hop, ...(redirected.record.redirects ?? [])],
      robots: [...policies, ...(redirected.record.robots ?? [])] };
    delete record.storage;
    await this._cache(record, redirected.body);
    return { record, body: redirected.body };
  }

  async _cache(record, body) {
    await writeFile(path.join(this.cacheDir, 'blobs/sha256', record.sha256), body, { flag: 'wx' })
      .catch(error => { if (error.code !== 'EEXIST') throw error; });
    this.entries.set(record.requestedUrl, record);
    await this._persist();
    // Successful bytes now live on disk; retain in-memory responses only until
    // persistence completes, or for failures that must not be retried.
    this.responses.delete(record.requestedUrl);
  }

  _persist() {
    this.indexWrite = this.indexWrite.then(() => atomicJson(path.join(this.cacheDir, 'index.json'), {
      schema: CACHE_SCHEMA, entries: Object.fromEntries(this.entries), aliases: [...this.aliases],
      failures: Object.fromEntries(this.failures),
    }));
    return this.indexWrite;
  }

  async _checkRobots(url, signal = this.lifecycle.signal) {
    const origin = new URL(url).origin;
    if (!this.policies.has(origin)) {
      const pending = (async () => {
        const robotsUrl = `${origin}/robots.txt`;
        const result = await this._fetchCanonical(robotsUrl, 'robots', { robots: true, signal });
        await this._record(result.record, result.body);
        if (![200, 404].includes(result.record.status)) {
          throw failure('robots-unavailable', `robots.txt returned HTTP ${result.record.status}: ${robotsUrl}`);
        }
        const parser = robotsParser(robotsUrl, result.record.status === 404 ? '' : result.body.toString('utf8'));
        const crawlDelay = parser.getCrawlDelay(USER_AGENT);
        if (Number.isFinite(crawlDelay) && crawlDelay >= 0) this.crawlDelays.set(origin, crawlDelay * 1000);
        return { parser, url: robotsUrl, sha256: result.record.sha256, status: result.record.status };
      })();
      this.policies.set(origin, pending);
    }
    let policy;
    try { policy = await this.policies.get(origin); }
    catch (error) { throw failure(error.code ?? 'robots-unavailable', `Robots policy unavailable for ${origin}: ${error.message}`); }
    if (policy.parser.isAllowed(url, USER_AGENT) === false) throw failure('robots-denied', `robots.txt disallows ${url}`);
    return { url: policy.url, sha256: policy.sha256, status: policy.status, userAgent: USER_AGENT };
  }

  async _queuedRequest(url, signal = this.lifecycle.signal, requestContext = ANONYMOUS_CONTEXT) {
    if (this.responses.has(url)) return this.responses.get(url);
    const origin = new URL(url).origin;
    const prior = this.queues.get(origin) ?? Promise.resolve();
    let started = false;
    const pending = prior.catch(() => {}).then(async () => {
      signal.throwIfAborted();
      const delay = Math.max(this.delayMs, this.crawlDelays.get(origin) ?? 0);
      try {
        await sleep(Math.max(0, (this.lastStarts.get(origin) ?? 0) + delay - Date.now()), undefined, { signal });
      } catch (error) { throw signal.aborted ? signal.reason : error; }
      signal.throwIfAborted();
      this.lastStarts.set(origin, Date.now());
      started = true;
      return this._request(url, signal, requestContext);
    }).catch(async error => {
      if (started && !(error.code === 'capture-cancelled' && error.requestStarted === false)) {
        const code = error.code === 'capture-cancelled' ? 'request-cancelled' : (error.code ?? 'fetch-error');
        this.failures.set(url, { code, message: error.message, failedAt: new Date().toISOString() });
        await this._persist();
        throw failure(code, error.message);
      }
      // A cancelled queue slot made no request. A later chart may still need it.
      this.responses.delete(url);
      throw error;
    });
    this.queues.set(origin, pending);
    this.responses.set(url, pending);
    return pending;
  }

  /** Cancel outstanding/queued browser traffic without disabling future charts. */
  cancelPending(message = 'Browser context closed before outstanding requests completed') {
    this.lifecycle.abort(failure('capture-cancelled', message));
    this.lifecycle = new AbortController();
    // Failed robots promises remain cached; unstarted, cancelled checks may run
    // when a future chart actually needs that origin.
    for (const [origin, pending] of this.policies) {
      pending.catch(error => {
        if (error.code === 'capture-cancelled' && this.policies.get(origin) === pending) this.policies.delete(origin);
      });
    }
  }

  async _request(value, lifecycleSignal = this.lifecycle.signal, requestContext = ANONYMOUS_CONTEXT) {
    const url = new URL(value);
    const deadline = new AbortController();
    const timer = setTimeout(() => deadline.abort(failure('request-timeout',
      `Request exceeded ${this.requestTimeoutMs}ms (including DNS and body): ${url}`)), this.requestTimeoutMs);
    const signal = AbortSignal.any([lifecycleSignal, deadline.signal]);
    let requestStarted = false;
    try {
      signal.throwIfAborted();
      const hostname = url.hostname.replace(/^\[|\]$/g, '');
      const addresses = isIP(hostname) ? [{ address: hostname, family: isIP(hostname) }]
        : await abortable(lookup(hostname, { all: true }), signal);
      signal.throwIfAborted();
      if (!addresses.length) throw failure('dns-empty', `No address for ${hostname}`);
      if (!this.allowedOrigins.has(url.origin) && addresses.some(({ address }) => privateAddress(address))) {
        throw failure('private-network', `Refusing a private or reserved network address: ${url.origin}`);
      }
      return await new Promise((resolve, reject) => {
        const transport = url.protocol === 'https:' ? https : http;
        requestStarted = true;
        const request = transport.get(url, {
          agent: false,
          signal,
          headers: { 'user-agent': USER_AGENT, 'accept-encoding': 'gzip, deflate, br', ...requestContext.headers },
          // Pin validated DNS results; never perform a second lookup/rebinding.
          lookup: (_host, options, callback) => options.all
            ? callback(null, addresses) : callback(null, addresses[0].address, addresses[0].family),
        }, async response => {
          try {
            const headers = filteredHeaders(response.headers);
            const encoding = String(response.headers['content-encoding'] ?? '').trim().toLowerCase();
            let stream = response;
            if (encoding && encoding !== 'identity') {
              const decoder = encoding === 'gzip' ? createGunzip() : encoding === 'deflate' ? createInflate()
                : encoding === 'br' ? createBrotliDecompress() : undefined;
              if (!decoder) throw failure('unsupported-encoding', `Unsupported Content-Encoding: ${encoding}`);
              stream = response.pipe(decoder);
              response.on('error', error => decoder.destroy(error));
            }
            const chunks = [];
            let length = 0;
            for await (const chunk of stream) {
              signal.throwIfAborted();
              length += chunk.length;
              if (length > MAX_BYTES) throw failure('resource-too-large', `Response exceeds ${MAX_BYTES} decoded bytes: ${url}`);
              chunks.push(chunk);
            }
            resolve({ status: response.statusCode, headers, body: Buffer.concat(chunks, length), requestContext });
          } catch (error) { request.destroy(); reject(signal.aborted ? signal.reason : error); }
        });
        request.on('error', error => reject(signal.aborted ? signal.reason : error));
      });
    } catch (error) {
      if (error.code === 'capture-cancelled') {
        throw Object.assign(failure(error.code, error.message), { requestStarted });
      }
      throw error;
    } finally { clearTimeout(timer); }
  }

  async attach(context) {
    this.contexts.add(context);
    context.on?.('close', () => {
      this.contexts.delete(context);
      if (!this.contexts.size) this.cancelPending();
    });
    await context.route('**/*', async route => {
      const request = route.request();
      if (!/^https?:/.test(request.url())) return route.continue();
      try {
        if (request.method() !== 'GET') throw failure('method-denied', `Only GET is allowed (${request.method()} ${request.url()})`);
        const actualHeaders = request.allHeaders ? await request.allHeaders() : request.headers?.() ?? {};
        const { record, body } = await this._fetchWithContext(request.url(),
          { role: request.resourceType(), followRedirects: false }, browserContext(actualHeaders));
        const redirect = record.redirects?.find(hop => hop.url === request.url());
        if (redirect) {
          // Let Chrome follow the original redirect so the document URL/base
          // and same-origin rules match the public page. Target bytes are cached.
          await route.fulfill({ status: redirect.status,
            headers: { ...redirect.headers, location: redirect.location, 'content-length': '0' }, body: '' });
        } else {
          await route.fulfill({ status: record.status,
            headers: { ...record.headers, 'content-length': String(body.length) }, body });
        }
      } catch (error) {
        if (!this.diagnostics.some(item => item.url === request.url() && item.code === error.code)) {
          this.diagnostics.push({ code: error.code ?? 'route-error', url: request.url(), message: error.message });
        }
        // Contexts can close while queued requests finish; do not leak rejections.
        await route.abort('blockedbyclient').catch(() => {});
      }
    });
    if (context.routeWebSocket) {
      await context.routeWebSocket('**/*', route => {
        this.diagnostics.push({ code: 'websocket-denied', url: route.url(),
          message: `WebSocket traffic is outside the static capture scope: ${route.url()}` });
        route.close();
      });
    }
    return this;
  }

  async save() {
    await Promise.allSettled([...this.inflight.values()]);
    if (!this.offline) await this._persist();
    const resources = { schema: RESOURCE_SCHEMA, resources: this.records, aliases: [...this.aliases],
      diagnostics: this.diagnostics, offlineMisses: this.offlineMisses,
      failures: Object.fromEntries(this.recordedFailures) };
    await atomicJson(path.join(this.outDir, 'resources.json'), resources);
    return resources;
  }
}
