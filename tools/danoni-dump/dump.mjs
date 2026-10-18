import { mkdir, readFile, writeFile, copyFile, access, mkdtemp, realpath, readdir, lstat } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { CaptureNetwork } from './network.mjs';
import { captureBrowser } from './browser.mjs';
import { hash, ADAPTER_ID } from './adapter.mjs';
import { decodeSnapshot, encodeSnapshot, projectSnapshot } from './serialization.mjs';
import { normalizeReference, validateReference, validateManifest, validateSchema } from './reference.mjs';

export const jsonWrite = async (filename, value) => {
  await mkdir(path.dirname(filename), { recursive: true });
  await writeFile(filename, JSON.stringify(value, null, 2) + '\n');
};
async function schemaWrite(filename, schema, value) {
  const validation = validateSchema(schema, value);
  if (!validation.valid) throw new Error(`Invalid ${schema}: ${JSON.stringify(validation.errors)}`);
  await jsonWrite(filename, value);
}
const jsonRead = async filename => JSON.parse(await readFile(filename, 'utf8'));
const pad = n => String(n).padStart(4, '0');

export async function requireExternalDirectory(directory) {
  let cursor = path.resolve(directory);
  while (true) {
    try {
      await access(path.join(cursor, '.git'));
      throw new Error('Downloaded data must stay outside a Git checkout: ' + directory);
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    try {
      const resolved = await realpath(cursor);
      if (resolved !== cursor) { await requireExternalDirectory(resolved); }
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    const parent = path.dirname(cursor);
    if (parent === cursor) return;
    cursor = parent;
  }
}

function relativeFile(root, file) {
  if (typeof file !== 'string' || path.isAbsolute(file) || file.includes('\\') || file.split('/').some(p => p === '..' || p === '.')) {
    throw new Error('Unsafe dump path: ' + file);
  }
  return path.join(root, file);
}

async function rejectSymlinks(root) {
  if ((await lstat(root)).isSymbolicLink()) throw new Error('Symlinks are not allowed in dumps: ' + root);
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const child = path.join(root, entry.name);
    if (entry.isSymbolicLink()) throw new Error('Symlinks are not allowed in dumps: ' + child);
    if (entry.isDirectory()) await rejectSymlinks(child);
  }
}

function roleFor(resource) {
  if (resource.mimeType?.startsWith('audio/')) return 'music';
  if (resource.mimeType?.startsWith('image/')) return 'image';
  if (resource.mimeType?.startsWith('font/') || /\.(woff2?|ttf|otf)(?:\?|$)/i.test(resource.requestedUrl)) return 'font';
  if (resource.mimeType?.includes('css')) return 'css';
  if (resource.mimeType?.includes('javascript') || resource.roles?.includes('script')) return 'script';
  return 'other';
}

export async function writeDump({ outDir, url, result, network, trustedHashes = [], termsNotes = '' }) {
  const at = new Date().toISOString();
  const scripts = [...new Map(result.charts.flatMap(c => c.scripts).map(s => [s.url + s.sha256, { url: s.url, sha256: s.sha256 }])).values()];
  const manifest = {
    schema: 'ddi-danoni-dump/1', page: { requestedUrl: url, finalUrl: result.charts[0]?.pageUrl ?? url, retrievedAt: at },
    engine: { name: 'danoniplus', version: result.catalog?.version ?? null, scriptHashes: scripts, adapterId: ADAPTER_ID },
    credits: { pageText: result.catalog?.creditText ?? '', byChart: [] }, charts: [], assets: [],
    status: 'partial', diagnostics: [], capturePath: 'capture.json', resourcesPath: 'resources.json',
  };
  const inputIndex = [];
  const mediaByUrl = new Map();
  for (const entry of result.charts) {
    const prefix = `charts/${pad(entry.chartId)}`;
    for (const input of entry.inputs) {
      const id = pad(inputIndex.length);
      const textPath = `inputs/${id}.dos.txt`, stringPath = `inputs/${id}.string.json`;
      await mkdir(path.join(outDir, 'inputs'), { recursive: true });
      await writeFile(path.join(outDir, textPath), input.text, 'utf8');
      await jsonWrite(path.join(outDir, stringPath), input.text);
      const { text, ...metadata } = input;
      inputIndex.push({ ...metadata, captureChartId: entry.chartId, textPath, stringPath });
    }
    const calls = entry.snapshots.filter(encoded => {
      const s = decodeSnapshot(encoded);
      return s.stage === 'scoreConvert-return-full' && s.chartId === entry.chartId;
    });
    const selected = calls.at(-1);
    const snapshot = selected ? decodeSnapshot(selected) : null;
    const globals = snapshot?.globals ?? {};
    const chart = {
      chartId: entry.chartId, sourceScoreNumber: globals.scoreNumber ?? null, sourceSuffix: globals.sourceSuffix ?? null,
      dosId: globals.dosId ?? null, musicId: globals.musicId ?? null,
      keyMode: snapshot?.header.keyLabels?.[entry.chartId] ?? null, defaultPatternId: globals.keyPattern ?? null,
      paths: { effectiveDos: null, keys: null, reference: null, snapshots: `${prefix}/engine-snapshots.json` },
      status: entry.status,
    };
    if (!entry.inputs.length || !entry.snapshots.some(s => {
      const decoded = decodeSnapshot(s); return decoded.stage === 'prepared' && decoded.chartId === entry.chartId;
    })) {
      chart.status = 'partial';
      entry.diagnostics.push({ code: 'missing-capture-stage', message: 'Exact dos inputs and requested-chart prepared snapshot are both required' });
    }
    await jsonWrite(path.join(outDir, chart.paths.snapshots), { schema: 'ddi-danoni-invocations/1', snapshots: entry.snapshots });
    if (selected) {
      chart.paths.effectiveDos = `${prefix}/effective-dos.json`;
      chart.paths.keys = `${prefix}/keys.json`;
      chart.paths.reference = `${prefix}/reference.json`;
      const reference = normalizeReference(selected, { engineScriptHashes: scripts, adapterId: ADAPTER_ID });
      const validation = validateReference(reference);
      if (!validation.valid) {
        chart.status = 'partial';
        entry.diagnostics.push({ code: 'invalid-reference', message: JSON.stringify(validation.errors) });
      }
      await jsonWrite(path.join(outDir, chart.paths.reference), reference);
      if (Object.values(snapshot.inputDos).some(v => typeof v !== 'string')) {
        chart.status = 'partial';
        entry.diagnostics.push({ code: 'non-string-dos', message: 'Effective dos contains non-string fields; inspect the native snapshot' });
      }
      await schemaWrite(path.join(outDir, chart.paths.effectiveDos), 'effective-dos', {
        schema: 'ddi-danoni-effective-dos/1', fields: Object.fromEntries(Object.entries(snapshot.inputDos).filter(([, value]) => typeof value === 'string')),
        native: projectSnapshot(selected, ['inputDos']),
      });
      await schemaWrite(path.join(outDir, chart.paths.keys), 'keys', { schema: 'ddi-danoni-keys/1', keyMode: chart.keyMode,
        activePattern: globals.keyPattern, lanes: globals.lanes, definitions: projectSnapshot(selected, ['key']) });
      manifest.credits.byChart.push({ chartId: entry.chartId,
        raw: Object.fromEntries(Object.entries(snapshot.inputDos).filter(([key]) => /^(musicTitle|tuning)(En|Ja)?$/.test(key))),
        resolved: encodeSnapshot({ musicTitles: snapshot.header.musicTitles, artistNames: snapshot.header.artistNames,
          artistUrls: snapshot.header.artistUrls, tuning: snapshot.header.tuning, creatorUrl: snapshot.header.creatorUrl,
          tuningNames: snapshot.header.tuningNames, tuningUrls: snapshot.header.tuningUrls,
          creator: snapshot.header.creatorNames?.[entry.chartId] }),
      });
    } else { chart.status = entry.diagnostics.some(d => d.code === 'unsupported-engine') ? 'unsupported' : 'partial'; }
    if (entry.media?.fullMusicUrl) {
      const list = mediaByUrl.get(entry.media.fullMusicUrl) ?? [];
      list.push(entry); mediaByUrl.set(entry.media.fullMusicUrl, list);
    }
    manifest.diagnostics.push(...entry.diagnostics.map(d => ({ ...d, chartId: entry.chartId })));
    manifest.charts.push(chart);
  }
  await schemaWrite(path.join(outDir, 'inputs/index.json'), 'inputs', { schema: 'ddi-danoni-inputs/1', inputs: inputIndex });
  for (const resource of network.records) {
    if (!(resource.status >= 200 && resource.status < 300) || !resource.sha256) continue;
    const musicEntries = mediaByUrl.get(resource.requestedUrl) ?? mediaByUrl.get(resource.finalUrl) ?? [];
    const asset = { id: `asset-${manifest.assets.length}`, role: musicEntries.length ? 'music' : roleFor(resource),
      sourceUrls: [resource.requestedUrl], sha256: resource.sha256, path: resource.bodyPath,
      mimeType: resource.mimeType ?? null, chartIds: musicEntries.map(c => c.chartId) };
    const decoded = musicEntries.find(c => c.media.decoded)?.media.decoded;
    if (decoded) {
      const bytes = Buffer.from(decoded.base64, 'base64');
      const digest = hash(bytes);
      const ext = ({ 'audio/wav': 'wav', 'audio/mpeg': 'mp3', 'audio/ogg': 'ogg', 'audio/flac': 'flac' })[decoded.mimeType] ?? 'bin';
      const mediaPath = `media/${digest}.${ext}`;
      await mkdir(path.join(outDir, 'media'), { recursive: true });
      await writeFile(path.join(outDir, mediaPath), bytes);
      Object.assign(asset, { decodedPath: mediaPath, decodedSha256: digest,
        extraction: { method: 'browser-musicInit-base64', mimeType: decoded.mimeType } });
    } else if (asset.role === 'music' && asset.mimeType?.startsWith('audio/')) {
      await mkdir(path.join(outDir, 'media'), { recursive: true });
      const ext = ({ 'audio/wav': 'wav', 'audio/x-wav': 'wav', 'audio/mpeg': 'mp3', 'audio/ogg': 'ogg' })[asset.mimeType] ?? 'bin';
      asset.decodedPath = `media/${asset.sha256}.${ext}`;
      asset.decodedSha256 = asset.sha256;
      asset.extraction = 'Byte-identical copy of captured audio response; no transcoding';
      await copyFile(path.join(outDir, asset.path), path.join(outDir, asset.decodedPath));
    }
    manifest.assets.push(asset);
  }
  for (const chart of manifest.charts) {
    if (!manifest.assets.some(a => a.role === 'music' && a.chartIds.includes(chart.chartId) && a.decodedPath)) {
      chart.status = 'partial';
      manifest.diagnostics.push({ code: 'missing-music', chartId: chart.chartId, message: 'No complete playable audio body mapped to this chart' });
    }
  }
  manifest.diagnostics.push(...network.diagnostics);
  if (network.diagnostics.some(d => !['cache-alias', 'route-closed'].includes(d.code))) {
    manifest.diagnostics.push({ code: 'network-limitations', message: 'One or more resources were denied or unavailable; see resource diagnostics' });
  }
  if (result.catalog && manifest.charts.length < result.catalog.count) {
    manifest.diagnostics.push({ code: 'chart-subset', message: `${manifest.charts.length} of ${result.catalog.count} author charts selected` });
  }
  // Complete only describes captured import data. No claim of full custom-code
  // gameplay emulation is made, even if the later offline check succeeds.
  if (manifest.charts.length && manifest.charts.every(c => c.status === 'complete') && !network.offlineMisses.length &&
      manifest.charts.length === result.catalog?.count && !manifest.diagnostics.some(d => d.code === 'network-limitations')) manifest.status = 'complete';
  await jsonWrite(path.join(outDir, 'capture.json'), { schema: 'ddi-danoni-capture/1', capturedAt: at,
    toolVersion: '0.1.0', browserVersion: result.browserVersion, trustedHashes, termsNotes,
    termsLinks: result.catalog?.termsLinks ?? [], totalAuthorCharts: result.catalog?.count ?? null,
    coverage: { selectedCharts: result.charts.map(c => c.chartId), stage: 'startup-through-main-entry',
      fullGameplayReplay: false, offlineVerified: false },
    charts: result.charts.map(c => ({ chartId: c.chartId, status: c.status, options: c.media?.effectiveOptions ?? null,
      viewport: c.media?.viewport ?? null, locale: c.media?.locale ?? null, customHooks: c.media?.customHooks ?? [], diagnostics: c.diagnostics })),
  });
  await network.save();
  const validation = validateManifest(manifest);
  if (!validation.valid) throw new Error('Internal manifest validation failed: ' + JSON.stringify(validation.errors));
  await jsonWrite(path.join(outDir, 'manifest.json'), manifest);
  return manifest;
}

export async function capture({ url, outDir, cacheDir, chartIds, trustedHashes = [], termsNotes = '', timeoutMs, onProgress, allowedOrigins = [], delayMs = 1000 }) {
  await requireExternalDirectory(outDir);
  await requireExternalDirectory(cacheDir ?? outDir);
  try { await rejectSymlinks(cacheDir ?? outDir); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  try {
    if ((await lstat(outDir)).isSymbolicLink() || (await readdir(outDir)).length) throw new Error('Dump output must be new or empty: ' + outDir);
  }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const network = await new CaptureNetwork({ outDir, cacheDir, allowedOrigins, delayMs }).init();
  const result = await captureBrowser({ url, network, chartIds, trustedHashes, timeoutMs, onProgress });
  return writeDump({ outDir, url, result, network, trustedHashes, termsNotes });
}

export async function verify(outDir, { onProgress, timeoutMs = 120000 } = {}) {
  await requireExternalDirectory(outDir);
  await rejectSymlinks(outDir);
  const manifest = await jsonRead(path.join(outDir, 'manifest.json'));
  const validation = validateManifest(manifest);
  if (!validation.valid) throw new Error('Invalid manifest: ' + JSON.stringify(validation.errors));
  if (!manifest.charts.length) throw new Error('No captured charts to verify');
  for (const asset of manifest.assets) {
    for (const [file, expected] of [[asset.path, asset.sha256], [asset.decodedPath, asset.decodedSha256]]) {
      if (file && hash(await readFile(relativeFile(outDir, file))) !== expected) throw new Error('Hash mismatch: ' + file);
    }
  }
  const captureInfo = await jsonRead(relativeFile(outDir, manifest.capturePath));
  const scratch = await mkdtemp(path.join(os.tmpdir(), 'danoni-verify-'));
  const network = await new CaptureNetwork({ cacheDir: outDir, outDir: scratch, offline: true }).init();
  const result = await captureBrowser({ url: manifest.page.requestedUrl, network,
    chartIds: manifest.charts.filter(c => c.paths.reference).map(c => c.chartId),
    trustedHashes: captureInfo.trustedHashes, timeoutMs, onProgress });
  const failures = [];
  const comparisons = [];
  for (const chart of manifest.charts) {
    if (!chart.paths.reference) { failures.push(`Chart ${chart.chartId}: no reference`); continue; }
    const expected = await jsonRead(relativeFile(outDir, chart.paths.reference));
    const replay = result.charts.find(c => c.chartId === chart.chartId);
    const call = replay?.snapshots.filter(s => {
      const decoded = decodeSnapshot(s);
      return decoded.stage === 'scoreConvert-return-full' && decoded.chartId === chart.chartId;
    }).at(-1);
    if (!call) { failures.push(`Chart ${chart.chartId}: replay failed`); comparisons.push({ chartId: chart.chartId, referenceMatched: false }); continue; }
    const actual = normalizeReference(call, { engineScriptHashes: manifest.engine.scriptHashes, adapterId: ADAPTER_ID });
    const referenceMatched = validateReference(actual).valid && JSON.stringify(actual) === JSON.stringify(expected);
    comparisons.push({ chartId: chart.chartId, referenceMatched });
    if (!referenceMatched) failures.push(`Chart ${chart.chartId}: reference differs`);
    if (replay.status !== 'complete') failures.push(`Chart ${chart.chartId}: replay incomplete`);
  }
  failures.push(...network.offlineMisses.map(m => `Offline cache miss: ${typeof m === 'string' ? m : JSON.stringify(m)}`));
  const report = { schema: 'ddi-danoni-verification/1', verifiedAt: new Date().toISOString(), passed: failures.length === 0,
    failures, comparisons, replayDirectory: scratch, coverage: 'captured chart startup, full converter returns, and observed assets',
    charts: result.charts.map(c => ({ chartId: c.chartId, status: c.status, diagnostics: c.diagnostics })) };
  await network.save();
  await jsonWrite(path.join(outDir, 'verification.json'), report);
  captureInfo.coverage.offlineVerified = report.passed;
  await jsonWrite(path.join(outDir, 'capture.json'), captureInfo);
  return report;
}
