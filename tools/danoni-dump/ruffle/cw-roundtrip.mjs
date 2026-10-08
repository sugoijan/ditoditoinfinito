// Validate a reviewed experimental export using a previously captured CW engine.
// Every browser request is fulfilled from local bytes; never access the network.
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { captureBrowser } from '../browser.mjs';
import { normalizeReference, validateReference } from '../reference.mjs';
import { decodeSnapshot } from '../serialization.mjs';
import { requireExternalDirectory } from '../dump.mjs';

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const [engineDir, exportDir, outDir] = process.argv.slice(2, 5).map(value => path.resolve(value));
const selectedId = process.argv[5];
if (!engineDir || !exportDir || !outDir) throw new Error('Usage: node cw-roundtrip.mjs ENGINE_DUMP EXPORT OUTPUT');
const manifest = JSON.parse(await readFile(path.join(exportDir, 'manifest.json')));
const legacy = manifest.schema === 'ddi-legacy-export/experimental-1';
if (!legacy && manifest.schema !== 'ddi-synthetic-export/experimental-1') throw new Error('Unsupported experimental export');
if (!selectedId && manifest.charts.length !== 1) throw new Error('Choose a chart id for a multi-chart export');
const exportedChart = selectedId ? manifest.charts.find(chart => chart.id === selectedId) : manifest.charts[0];
if (!exportedChart) throw new Error('Selected chart is absent');
const exportFile = value => {
  if (typeof value !== 'string' || value.includes('\\') || value.startsWith('/') || value.split('/').some(p => !p || p === '.' || p === '..')) throw new Error('Invalid export-relative path');
  return path.join(exportDir, value);
};
await requireExternalDirectory(outDir);
await mkdir(outDir, { recursive: false });
const resources = JSON.parse(await readFile(path.join(engineDir, 'resources.json')));
const entries = new Map();
const sourceEvidence = [];
for (const record of resources.resources) {
  const pathname = new URL(record.url).pathname;
  if (!/^\/(js|css|skin|img)\//.test(pathname) || record.status !== 200 || !record.completeBody) continue;
  if (record.bodyPath !== `blobs/sha256/${record.sha256}` || !/^[a-f0-9]{64}$/.test(record.sha256)) throw new Error('Invalid cached blob path');
  const body = await readFile(path.join(engineDir, record.bodyPath));
  if (sha256(body) !== record.sha256) throw new Error(`Corrupt cached engine resource: ${record.url}`);
  if (entries.has(pathname) && entries.get(pathname).record.sha256 !== record.sha256) throw new Error(`Ambiguous cached engine versions: ${pathname}`);
  entries.set(pathname, { record, body });
  sourceEvidence.push({ url: record.url, sha256: record.sha256, retrievedAt: record.retrievedAt });
}
const dos = await readFile(exportFile(exportedChart.dos), 'utf8');
const escape = text => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const html = `<!doctype html><meta charset="utf-8"><title>CW export comparison</title>
<script src="/js/danoni_main.js"></script><link rel="stylesheet" href="/css/danoni_main.css">
<textarea hidden id="dos">${escape(dos)}</textarea><div id="canvas-frame" style="width:650px;margin:auto"></div>`;
const music = exportedChart.music ?? manifest.music ?? { path: 'media/synthetic.wav', mimeType: 'audio/wav' };
const musicBody = await readFile(exportFile(music.path));
if (music.sha256 && sha256(musicBody) !== music.sha256) throw new Error('Corrupt exported music');
const generated = new Map([
  [`/${exportedChart.page}`, { body: Buffer.from(html), mimeType: 'text/html' }],
  [`/${music.path}`, { body: musicBody, mimeType: music.mimeType }],
]);
const misses = [];
const origin = 'https://ddi-ruffle.invalid';
const network = {
  inflight: new Map(),
  addAlias() {}, // Paths have already been checked for ambiguous cached versions.
  async fetch(value) {
    const url = new URL(value);
    const generatedEntry = generated.get(url.pathname);
    const cached = entries.get(url.pathname);
    if (url.origin !== origin || (!generatedEntry && !cached)) {
      misses.push(value);
      throw new Error(`Offline engine resource unavailable: ${value}`);
    }
    return generatedEntry ? { body: generatedEntry.body, record: { status: 200, completeBody: true, mimeType: generatedEntry.mimeType } } : cached;
  },
  async attach(context) {
    await context.routeWebSocket('**/*', socket => {
      misses.push(socket.url());
      socket.close();
    });
    await context.route('**/*', async route => {
      try {
        const { body, record } = await this.fetch(route.request().url());
        await route.fulfill({ status: 200, body, contentType: record.mimeType });
      } catch { await route.abort('blockedbyclient'); }
    });
  },
};
const capture = await captureBrowser({ url: `${origin}/${exportedChart.page}`, network, chartIds: [0], timeoutMs: 15000, onProgress: console.error });
await writeFile(path.join(outDir, 'capture.json'), JSON.stringify(capture, null, 2));
const observed = capture.charts[0];
const snapshot = observed.snapshots.filter(encoded => decodeSnapshot(encoded).stage === 'scoreConvert-return-full').at(-1);
if (!snapshot) throw new Error(`No full CW parse: ${JSON.stringify(observed.diagnostics)}; misses: ${misses.join(', ')}`);
const reference = normalizeReference(snapshot, { engineScriptHashes: observed.scripts.map(script => ({ url: script.url, sha256: script.sha256 })), adapterId: 'cw-lexical-cdp/1' });
await writeFile(path.join(outDir, 'reference.json'), JSON.stringify(reference, null, 2));
const validation = validateReference(reference);
const expected = exportedChart;
const discrepancies = [];
let maxErrorSeconds = 0;
// This fixture deliberately needs no padding. Refuse an unexpected origin shift.
const shift = legacy ? reference.timing.intAdjustment : 0;
if ((!legacy && (reference.timing.intAdjustment !== 0 || reference.timing.preblankFrame !== 0)) ||
  (legacy && (reference.timing.headerAdjustment !== 0 || reference.timing.userAdjustment !== 0 ||
    reference.timing.playbackRate !== 1 || shift !== reference.timing.preblankFrame))) {
  discrepancies.push({ kind: 'unexpected-timing-transform', timing: reference.timing });
}
for (const lane of expected.lanes) {
  const actual = reference.lanes.find(value => value.chara === lane.name);
  if (!actual) { discrepancies.push({ kind: 'missing-lane', lane: lane.name }); continue; }
  for (const [kind, want, got] of [['taps', lane.tapsSeconds, actual.taps.map(f => (f - shift) / 60)],
    ['holds', lane.holdsSeconds.flat(), actual.holds.flat().map(f => (f - shift) / 60)]]) {
    if (want.length !== got.length) discrepancies.push({ kind: 'count', lane: lane.name, event: kind, expected: want.length, actual: got.length });
    for (let i = 0; i < Math.min(want.length, got.length); i++) {
      const errorSeconds = Math.abs(want[i] - got[i]);
      maxErrorSeconds = Math.max(maxErrorSeconds, errorSeconds);
      if (errorSeconds > 1e-12) discrepancies.push({ kind: 'timing', lane: lane.name, event: kind, index: i, expectedSeconds: want[i], actualSeconds: got[i] });
    }
  }
}
for (const [kind, want, got] of [['speed', expected.speed ?? [], reference.speed],
  ['colors', expected.colors ?? [], reference.colors.legacy]]) {
  if (!legacy) continue;
  if (want.length !== got.length) discrepancies.push({ kind: 'event-count', event: kind, expected: want.length, actual: got.length });
  for (let i = 0; i < Math.min(want.length, got.length); i++) {
    const errorSeconds = Math.abs(want[i].seconds - (got[i].frame - shift) / 60);
    maxErrorSeconds = Math.max(maxErrorSeconds, errorSeconds);
    if (errorSeconds > 1e-12) discrepancies.push({ kind: 'event-timing', event: kind, index: i, errorSeconds });
    for (const field of kind === 'speed' ? ['multiplier'] : ['target', 'color', 'all']) {
      if (want[i][field] !== got[i][field]) discrepancies.push({ kind: 'event-value', event: kind, index: i, field, expected: want[i][field], actual: got[i][field] });
    }
  }
}
const report = { schema: 'ddi-cw-roundtrip/experimental-1', passed: validation.valid && observed.status === 'complete' && !misses.length && !discrepancies.length,
  chartId: exportedChart.id, sourceSnapshotSha256: exportedChart.sourceSnapshotSha256 ?? manifest.sourceSnapshotSha256, exportDosSha256: sha256(Buffer.from(dos)), engine: sourceEvidence,
  browserVersion: capture.browserVersion, maxErrorSeconds, discrepancies, misses, referenceValidation: validation,
  diagnostics: observed.diagnostics, cwAudioRelativeFrameShift: shift,
  scope: legacy ? 'Selected legacy static notes, holds, speed and expanded arrow colors; original Flash audio latency and gameplay fidelity unmeasured' : 'Synthetic notes and holds only; does not validate a legacy engine family or effects' };
await writeFile(path.join(outDir, 'conversion.json'), JSON.stringify(report, null, 2));
console.log(JSON.stringify({ passed: report.passed, maxErrorSeconds, discrepancies, misses, referenceValidation: validation }, null, 2));
if (!report.passed) process.exitCode = 2;
