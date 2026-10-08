import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { mergeCaches } from '../merge-cache.mjs';
import { CaptureNetwork } from '../network.mjs';

async function cache(root, name, records, failures = {}, aliases = []) {
  const directory = path.join(root, name);
  await mkdir(path.join(directory, 'blobs/sha256'), { recursive: true });
  const entries = {};
  for (const [url, text] of records) {
    const bytes = Buffer.from(text), sha256 = createHash('sha256').update(bytes).digest('hex');
    entries[url] = { requestedUrl: url, url, finalUrl: url, status: 200, sha256,
      bodyPath: `blobs/sha256/${sha256}`, byteLength: bytes.length, headers: {}, completeBody: true, retrievedAt: name };
    await writeFile(path.join(directory, entries[url].bodyPath), bytes);
  }
  await writeFile(path.join(directory, 'index.json'), JSON.stringify({ schema: 'ddi-danoni-cache/1', entries, failures, aliases }));
  return directory;
}

test('merge preserves bodies, provenance, aliases and refusals for offline reuse', async t => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'ddi-merge-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const source = await cache(root, 'source', [['https://example.invalid/shared', 'same'], ['https://example.invalid/new', 'new']],
    { 'https://example.invalid/refused': { code: 'robots-denied', message: 'Original refusal' } },
    [['https://example.invalid/new?t=1', 'https://example.invalid/new']]);
  const target = await cache(root, 'target', [['https://example.invalid/shared', 'same']]);
  const originalSource = await readFile(path.join(source, 'index.json'));
  const report = await mergeCaches(source, target);
  assert.equal(report.networkRequests, 0);
  assert.equal(report.imported.length, 1);
  assert.equal(report.duplicates[0].sourceRetrievedAt, 'source');
  assert.equal(report.duplicates[0].targetRetrievedAt, 'target');
  assert.deepEqual(await readFile(path.join(source, 'index.json')), originalSource);
  const network = await new CaptureNetwork({ cacheDir: target, outDir: path.join(root, 'replay'), offline: true }).init();
  const response = await network.fetch('https://example.invalid/new?t=1');
  assert.equal(response.body.toString(), 'new'); assert.equal(response.record.fromCache, true);
  await assert.rejects(network.fetch('https://example.invalid/refused'), { code: 'robots-denied' });
});

test('merge refuses conflicting bodies and Git output before changing the index', async t => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'ddi-merge-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const source = await cache(root, 'source', [['https://example.invalid/shared', 'new version']]);
  const target = await cache(root, 'target', [['https://example.invalid/shared', 'original']]);
  const before = await readFile(path.join(target, 'index.json'));
  await assert.rejects(mergeCaches(source, target), /Conflicting cached response/);
  assert.deepEqual(await readFile(path.join(target, 'index.json')), before);
  await mkdir(path.join(target, '.git'));
  await assert.rejects(mergeCaches(source, target), /Git checkout/);
});
