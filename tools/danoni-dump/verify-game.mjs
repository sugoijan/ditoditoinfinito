// Verify reviewed local exports in a separately built game. Corpus specs and
// all third-party data/screenshots stay outside Git; see danoni-runtime.md.
import { readFile, writeFile, mkdir, realpath } from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { chromium } from 'playwright-core';
import { requireExternalDirectory } from './dump.mjs';
import { installProbe, audioObservation, fitMotion, observeUnactivatedPreview } from './game-probe.mjs';

const [url, specFile, output] = process.argv.slice(2);
if (!url || !specFile || !output || !['localhost', '127.0.0.1', '[::1]'].includes(new URL(url).hostname)) throw Error('Usage: node verify-game.mjs LOOPBACK_URL EXTERNAL_SPEC_JSON EXTERNAL_OUTPUT');
const externalFile = async file => requireExternalDirectory(path.dirname(await realpath(file)));
await externalFile(specFile); await requireExternalDirectory(output);
const specBytes = await readFile(specFile), spec = JSON.parse(specBytes);
if (spec.schema !== 'ddi-danoni-game-check/1') throw Error('Unknown verification spec');
const layouts = new Set(['danoni-5', 'danoni-7', 'danoni-9A', 'danoni-9B']);
if (!spec.modes.every(m => layouts.has(m.layout)) || ![...spec.play, spec.scroll].every(t => layouts.has(t.layout) && Number.isInteger(t.chartIndex) && t.chartIndex >= 0)) throw Error('Invalid supported-mode target');
const phases = spec.phases ?? ['play', 'scroll', 'previews'];
if (!phases.length || phases.some(p => !['play', 'scroll', 'previews'].includes(p))) throw Error('Invalid verification phases');
await mkdir(output, { recursive: false });
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const report = { schema: 'ddi-danoni-game-verification/1', passed: false, startedAt: new Date().toISOString(),
  url, phases, specSha256: hash(specBytes), runnerSha256: hash(await readFile(new URL(import.meta.url))),
  probeSha256: hash(await readFile(new URL('./game-probe.mjs', import.meta.url))), imports: [], play: [], scroll: [], previews: [], decoderMessages: [], errors: [] };
const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-unsafe-webgpu', '--use-angle=metal', '--autoplay-policy=document-user-activation-required'] });
let context;
const save = () => writeFile(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
try {
  report.browserVersion = browser.version();
  context = await browser.newContext({ viewport: { width: 1280, height: 960 }, deviceScaleFactor: 1 });
  await context.addInitScript(installProbe);
  await context.addInitScript(() => { delete window.showDirectoryPicker; });
  const page = await context.newPage();
  page.on('pageerror', e => report.errors.push(String(e)));
  page.on('console', m => { if (m.type() === 'error') report.errors.push(m.text()); if (/decoded in wasm|built-in decoder/i.test(m.text())) report.decoderMessages.push(m.text()); });
  await page.goto(url);
  for (const pack of spec.packs) {
    await externalFile(pack.path);
    const artifact = { path: pack.path, sha256: hash(await readFile(pack.path)) };
    await page.locator('.import-panel input[type=file]').nth(1).setInputFiles(pack.path);
    await page.waitForFunction(() => /imported|failed|error/i.test(document.querySelector('.import-status')?.textContent ?? '') && !/importing|reading/i.test(document.querySelector('.import-status')?.textContent ?? ''), null, { timeout: 90000 });
    const status = await page.locator('.import-status').textContent(), warnings = await page.locator('.import-warnings li').allTextContents();
    if (!status.includes(`Imported ${pack.songs} songs`) || warnings.length) throw Error('Unexpected import outcome: ' + JSON.stringify({ status, warnings }));
    report.imports.push({ ...artifact, status, warnings });
  }
  const songs = await page.evaluate(() => new Promise((resolve, reject) => {
    const r = indexedDB.open('ddi-library'); r.onerror = () => reject(String(r.error));
    r.onsuccess = () => { const db = r.result, q = db.transaction('songs').objectStore('songs').getAll();
      q.onerror = () => reject(String(q.error)); q.onsuccess = () => { db.close(); resolve(q.result.map(x => JSON.parse(x))); }; };
  }));
  const charts = songs.flatMap(s => s.entry.charts);
  if (songs.length !== spec.songs || charts.length !== spec.charts || charts.reduce((n, c) => n + c.notes, 0) !== spec.heads) throw Error('Imported corpus counts disagree');
  if (charts.some(c => !spec.modes.some(m => c.layout === m.layout))) throw Error('Unexpected/unsupported layout');
  const rated = songs.filter(s => s.pack === spec.ratedPack).flatMap(s => s.entry.charts).filter(c => c.meter > 0);
  if (rated.length !== spec.ratedCharts || rated.some(c => c.difficulty !== (c.meter <= 1 ? 'Beginner' : c.meter <= 3 ? 'Easy' : c.meter <= 5 ? 'Medium' : c.meter <= 7 ? 'Hard' : 'Challenge'))) throw Error('Published meters/tiers disagree');
  if (songs.some(s => !s.entry.bpm || s.entry.bpm === '75' || !s.entry.banner)) throw Error('Missing fitted tempo/banner');
  await writeFile(path.join(output, 'imported-songs.json'), JSON.stringify(songs, null, 2));
  const later = page.getByRole('button', { name: 'later', exact: true }); if (await later.count()) await later.click();
  const decoded = new Set();
  for (const mode of spec.modes) {
    await page.getByRole('button', { name: mode.button, exact: true }).click();
    if (songs.filter(s => s.entry.charts.some(c => c.layout === mode.layout)).length !== mode.songs) throw Error('Wrong mode song count');
    for (const [index, pack] of spec.packs.entries()) {
      const expected = songs.filter(s => s.pack === pack.title && s.entry.charts.some(c => c.layout === mode.layout)).length;
      if (!expected) continue;
      const header = page.locator('.pack-header').filter({ hasText: pack.title });
      if (await header.getAttribute('aria-expanded') === 'false') await header.click();
      await page.waitForFunction(count => document.querySelectorAll('.song-card').length === count, expected);
      for (const img of await page.locator('img.song-banner[src^="blob:"]').all()) {
        await img.scrollIntoViewIfNeeded(); await img.evaluate(i => i.decode()); decoded.add(await img.getAttribute('src'));
      }
      await page.screenshot({ path: path.join(output, `menu-${mode.layout}-${index}.png`), fullPage: true });
    }
  }
  if (decoded.size !== spec.songs) throw Error('Not all imported banners decoded');
  report.menu = { songs: songs.length, charts: charts.length, heads: spec.heads, decodedBanners: decoded.size, ratedCharts: rated.length };
  const imported = target => {
    const matches = songs.filter(s => s.pack === target.pack && s.entry.title === target.title);
    if (matches.length !== 1) throw Error('Target song identity is ambiguous: ' + target.title);
    const s = matches[0], c = s.entry.charts.find(c => c.index === target.chartIndex && c.layout === target.layout);
    if (!c) throw Error('Missing selected chart: ' + target.title);
    return { songId: s.entry.id, chartIndex: c.index };
  };
  const start = async (target, backend, speed, motion = false) => {
    const ids = target.songId ? target : imported(target);
    await page.goto(url); await page.evaluate(speed => {
      const old = JSON.parse(localStorage.getItem('ddi.settings.v1') ?? '{}');
      localStorage.setItem('ddi.settings.v1', JSON.stringify({ ...old, speed, speed_source: 'Player', reverse: false,
        scroll_action: 'Normal', chart_colors: false, visual_offset: 0, audio_offset: 0, noteskin: null }));
    }, speed);
    await page.goto(`${url}#/play?song=${encodeURIComponent(ids.songId)}&chart=${ids.chartIndex}&auto=1${backend === 'gl' ? '&gfx=gl' : ''}`);
    await page.reload();
    if (motion) await page.evaluate(() => { window.__DDI_CORPUS_PROBE.captureMotion = true; });
    await page.locator('.start-prompt').click({ timeout: 30000 });
    await page.waitForFunction(() => window.__DDI_DEBUG?.song_time < 1 && window.__DDI_DEBUG?.frames < 200, null, { timeout: 15000 });
  };
  for (const backend of (phases.includes('play') ? ['gpu', 'gl'] : [])) {
    for (const target of spec.play) {
      await start(target, backend, spec.playSpeed);
      await page.waitForFunction(duration => window.__DDI_DEBUG?.song_time >= duration, target.seconds, { timeout: (target.seconds + 20) * 1000 });
      const state = await page.evaluate(() => window.__DDI_DEBUG);
      if (state.failed || !state.taps[0] || state.taps.slice(1).some(n => n) || state.let_go || state.held < (target.minimumHolds ?? 0)) throw Error('Autoplay failed: ' + JSON.stringify({ target, state }));
      if (state.noteskin !== null) throw Error('Expected procedural fallback for non-dance DanOni lanes');
      if ((backend === 'gl') !== /WebGL/i.test(state.backend)) throw Error('Wrong renderer backend');
      await page.screenshot({ path: path.join(output, `play-${target.layout}-${target.chartIndex}-${backend}.png`) });
      report.play.push({ target, backend, speed: spec.playSpeed, state }); await save(); console.log(backend, target.layout, target.title, state.taps[0], 'W1', state.held, 'holds');
    }
    await start(spec.danceControl, backend, spec.playSpeed);
    await page.waitForFunction(() => window.__DDI_DEBUG?.song_time >= 8, null, { timeout: 25000 });
    const state = await page.evaluate(() => window.__DDI_DEBUG);
    if (state.noteskin?.name !== 'Dito Vivid' || !state.noteskin.lua.runs || state.noteskin.lua.stopped || state.noteskin.problems.length || !state.noteskin.quads || state.failed || state.taps.slice(1).some(n => n)) throw Error('Default dance skin control failed: ' + JSON.stringify(state));
    await page.screenshot({ path: path.join(output, `dito-control-${backend}.png`) });
    report.play.push({ target: spec.danceControl, backend, state });
  }
  for (const speed of (phases.includes('scroll') ? [1, 2, 3] : [])) {
    await start(spec.scroll, 'gpu', speed, true);
    await page.waitForFunction(until => window.__DDI_DEBUG?.song_time >= until, spec.scroll.captureUntilSeconds, { timeout: 10000 });
    const observed = await page.evaluate(() => ({ motion: window.__DDI_CORPUS_PROBE.motion, failures: window.__DDI_CORPUS_PROBE.failures }));
    if (observed.failures.length) throw Error(observed.failures.join('\n'));
    // First head only: exclude the receptor-crossing/replacement period. The
    // inspected clock publishes every 20 frames, so use host-time intervals.
    const frames = observed.motion.filter(f => Number.isFinite(f.debugTime) && f.debugTime >= spec.scroll.publishedTimeWindow[0] && f.debugTime < spec.scroll.publishedTimeWindow[1]);
    const fit = fitMotion(frames), expected = spec.scroll.baseArrowHeightsPerSecond * speed;
    if (Math.abs(fit.arrowHeightsPerSecond / expected - 1) > .05) throw Error('Native scroll multiplier disagrees: ' + JSON.stringify({ speed, expected, fit }));
    report.scroll.push({ speed, expectedArrowHeightsPerSecond: expected, ...fit }); await save();
  }
  for (const target of (phases.includes('previews') ? spec.previews : [])) {
    await page.goto(url); await page.reload();
    await page.getByRole('button', { name: target.modeButton, exact: true }).click();
    const header = page.locator('.pack-header').filter({ hasText: target.pack });
    if (await header.getAttribute('aria-expanded') === 'false') await header.click();
    const matches = songs.filter(s => s.pack === target.pack && s.entry.title === target.title);
    if (matches.length !== 1) throw Error('Preview song identity ambiguous');
    const song = matches[0];
    const card = page.locator('.song-card').filter({ has: page.locator(`.chart-button[href*="song=${encodeURIComponent(song.entry.id)}&"]`) });
    if (await card.count() !== 1) throw Error('Preview identity ambiguous');
    await page.evaluate(wasm => { window.__DDI_FORCE_WASM_DECODE = wasm; }, target.wasm ?? false);
    await card.locator('.preview-button').click();
    await page.waitForFunction(() => window.__DDI_CORPUS_PROBE.starts.length > 0, null, { timeout: 30000 });
    await page.waitForTimeout(1200);
    const observation = await page.evaluate(audioObservation), latest = observation.contexts.at(-1), start = observation.starts.at(-1);
    if (target.wasm && !report.decoderMessages.some(m => /decoded in wasm/i.test(m))) throw Error('Forced decoder was not observed');
    if (!latest.createdActive || latest.state !== 'running' || latest.peak < .005 || Math.abs(start.offset - song.entry.preview_start) > 1e-6) throw Error('Gesture-gated preview failed: ' + JSON.stringify(observation));
    report.previews.push({ target, expectedOffsetSeconds: song.entry.preview_start, observation });
    await card.locator('.preview-button').click(); await save();
  }
  // Fresh document with no previous user activation: a script click must not
  // produce playing audio under Chrome's strict gesture policy.
  if (phases.includes('previews')) {
  const negativeContext = await browser.newContext();
  try {
    report.previewNegativeControl = await observeUnactivatedPreview(negativeContext, url);
  } finally { await negativeContext.close(); }
  }
  if (report.errors.length) throw Error(report.errors.join('\n'));
  report.passed = true; await save(); console.log(JSON.stringify({ passed: true, playRuns: report.play.length, scrollRuns: report.scroll.length, previews: report.previews.length, output }));
} catch (error) { report.failure = String(error); await save(); throw error; }
finally { if (context) await context.close(); await browser.close(); }
