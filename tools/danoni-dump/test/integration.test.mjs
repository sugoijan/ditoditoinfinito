import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { capture, verify } from '../dump.mjs';
import { decodeSnapshot } from '../serialization.mjs';
import { startFixture, silentWave } from './fixture.mjs';

const json = async filename => JSON.parse(await readFile(filename, 'utf8'));

for (const options of [{}, { musicMenu: true }, { missingImage: true }]) {
  const description = options.missingImage ? 'missing-image charts as partial'
    : options.musicMenu ? 'custom music-menu charts' : 'ordinary charts';
  test(`Chrome captures ${description} and replays with the origin shut down`,
    { timeout: 90000 }, () => captureAndReplay(options));
}

async function captureAndReplay({ musicMenu = false, missingImage = false }) {
  const scratch = await mkdtemp(path.join(os.tmpdir(), 'ddi-danoni-integration-'));
  const outDir = path.join(scratch, 'dump');
  const fixture = await startFixture({ musicMenu, missingImage });
  const expectedStatus = missingImage ? 'partial' : 'complete';
  let closed = false;
  let replayDirectory;
  try {
    const manifest = await capture({
      url: fixture.url, outDir, cacheDir: path.join(scratch, 'independent-cache'),
      allowedOrigins: [fixture.url], delayMs: 0,
      trustedHashes: [fixture.engineSha256], timeoutMs: 15000,
    });
    assert.equal(manifest.status, expectedStatus, JSON.stringify(manifest.diagnostics));
    assert.deepEqual(manifest.charts.map(chart => [chart.chartId, chart.status]), [[0, expectedStatus], [1, expectedStatus]]);
    if (missingImage) {
      assert(manifest.diagnostics.some(item => /Required visual resource is unavailable: HTTP 404/.test(item.message)));
    }
    const { inputs } = await json(path.join(outDir, 'inputs/index.json'));
    assert.equal(inputs.length, 4);
    const expected = [
      { taps: [[226, 236], [250, 260]], holds: [[[245, 255]], []] },
      { taps: [[261], [270]], holds: [[], [[280, 290]]] },
    ];
    for (const chart of manifest.charts) {
      const chartInputs = inputs.filter(input => input.captureChartId === chart.chartId);
      assert.equal(chartInputs.length, 2);
      const strings = await Promise.all(chartInputs.map(input => json(path.join(outDir, input.stringPath))));
      assert.match(strings[0], /\|left_data=210,230\|/);
      assert.equal(strings[1], '|right_data=260,280|tuning=Generated fixture,https://example.invalid/|');
      for (let i = 0; i < chartInputs.length; i++) {
        assert.equal(await readFile(path.join(outDir, chartInputs[i].textPath), 'utf8'), strings[i]);
      }
      const invocations = await json(path.join(outDir, chart.paths.snapshots));
      const snapshots = invocations.snapshots.map(decodeSnapshot);
      const calls = snapshots.filter(snapshot => snapshot.stage === 'scoreConvert-return-full');
      assert.equal(calls.length, 2);
      assert.deepEqual(calls.map(call => call.globals.preblankFrame), [0, 20]);
      assert(snapshots.some(snapshot => snapshot.stage === 'prepared'));
      assert(snapshots.some(snapshot => snapshot.stage === 'runtime'));
      const effective = await json(path.join(outDir, chart.paths.effectiveDos));
      assert.equal(effective.fields.left_data, '212,232');
      assert.equal(effective.fields.left2_data, '282');
      assert.equal(effective.fields.right_data, '260,280');
      if (musicMenu) {
        // Music IDs intentionally differ from chart IDs. Both DOM events must
        // have run; assigning engine globals alone cannot satisfy these checks.
        assert.equal(effective.fields.menuSelection, String(1 - chart.chartId));
        assert.equal(effective.fields.menuAccepted, 'true');
        assert.equal(calls.at(-1).state.menuInputEvents, 1);
        assert.equal(calls.at(-1).state.menuAcceptEvents, 1);
        assert.equal(chart.musicId, 1 - chart.chartId);
      }
      const reference = await json(path.join(outDir, chart.paths.reference));
      assert.deepEqual(reference.lanes.map(lane => lane.taps), expected[chart.chartId].taps);
      assert.deepEqual(reference.lanes.map(lane => lane.holds), expected[chart.chartId].holds);
      assert.equal(reference.patternId, 'Tiny_0');
      assert.equal(reference.timing.preblankFrame, 20);
      assert.equal(reference.timing.playbackRate, 2);
      assert.equal(reference.timing.realAdjustment, 20.75);
      assert.deepEqual(reference.speed, [{ frame: 220, multiplier: 1 }, { frame: 250, multiplier: 2 }]);
      assert.deepEqual(reference.boost, [{ frame: 220, multiplier: 1 }, { frame: 260, multiplier: 0.5 }]);
      assert.deepEqual(reference.colors.legacy, [{ frame: 230, target: 0, color: '#ff0000', all: false }]);
      const keys = await json(path.join(outDir, chart.paths.keys));
      assert.deepEqual(decodeSnapshot(keys.definitions).charaTiny_0, ['left', 'right']);
      assert.deepEqual(keys.lanes.map(lane => lane.tapDataKey), chart.chartId === 0
        ? ['left_data', 'right_data'] : ['left2_data', 'right2_data']);
    }
    const tracks = manifest.assets.filter(asset => asset.role === 'music');
    assert.equal(tracks.length, musicMenu ? 2 : 1);
    for (const chart of manifest.charts) {
      const music = tracks.find(asset => asset.chartIds.includes(chart.chartId));
      assert(music?.decodedPath, 'Encoded music has a decoded artifact');
      const filename = musicMenu && chart.chartId === 0 ? './music-second.js' : './music.js';
      assert.deepEqual(music.sourceUrls, [new URL(filename, fixture.url).href]);
      assert.deepEqual(await readFile(path.join(outDir, music.decodedPath)), silentWave());
    }
    const image = manifest.assets.find(asset => asset.role === 'image');
    if (missingImage) {
      assert.equal(image, undefined, 'A 404 response is not a captured image asset');
    } else {
      assert(image?.sourceUrls.includes(new URL('./img/pixel.svg', fixture.url).href));
      assert.match(await readFile(path.join(outDir, image.path), 'utf8'), /<svg/);
    }

    // Every fetched path, including cache-busted dos.js, is served once.
    const fetchedPaths = fixture.requests.map(url => url.split('?')[0]);
    assert.equal(new Set(fetchedPaths).size, fetchedPaths.length, JSON.stringify(fixture.requests));
    const requestCount = fixture.requests.length;
    await fixture.close();
    closed = true;
    const report = await verify(outDir, { timeoutMs: 15000 });
    replayDirectory = report.replayDirectory;
    assert.equal(report.passed, !missingImage, JSON.stringify(report));
    assert.deepEqual(report.comparisons, [
      { chartId: 0, referenceMatched: true }, { chartId: 1, referenceMatched: true },
    ]);
    assert.deepEqual(report.charts.map(chart => chart.status), [expectedStatus, expectedStatus]);
    if (missingImage) {
      assert(report.failures.some(message => /replay incomplete/.test(message)), 'Matching references cannot make missing assets pass');
    }
    assert.equal(fixture.requests.length, requestCount);
    assert.equal((await json(path.join(outDir, 'capture.json'))).coverage.offlineVerified, !missingImage);
  } finally {
    if (!closed) await fixture.close();
    await rm(scratch, { recursive: true, force: true });
    if (replayDirectory) await rm(replayDirectory, { recursive: true, force: true });
  }
}
