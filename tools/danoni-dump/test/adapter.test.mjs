import test from 'node:test';
import assert from 'node:assert/strict';
import { EngineObserver, hash } from '../adapter.mjs';

function observerFor(source, policy = undefined) {
  const observer = new EngineObserver();
  const scriptHash = hash(source);
  observer.trusted = new Set();
  observer.loaderPolicies = new Map(policy ? [[scriptHash, policy]] : []);
  observer.scriptUrls = new Map([['1', 'https://example.org/js/loader.js']]);
  observer.cdp = { async send(method) {
    if (method === 'Debugger.getScriptSource') return { scriptSource: source };
    if (method === 'Debugger.resume') return {};
    throw Error(`Unexpected CDP operation: ${method}`);
  } };
  return observer;
}

const paused = { reason: 'instrumentation', data: { scriptId: '1' }, callFrames: [] };

test('reviewed loader activates only its exact probe URL before engine recognition', async () => {
  const source = 'const syntheticLoader = 1;';
  const observer = observerFor(source, { versionProbePath: './version.txt', url: 'https://example.org/js/loader.js' });
  await observer.onPause(paused);
  assert.deepEqual([...observer.cacheBusterUrls], ['https://example.org/js/version.txt']);
  assert.equal(observer.scripts.length, 0, 'a selector is not an engine-version script');
  assert.equal(observer.loaderScripts[0].sha256, hash(source));
  assert.equal(observer.diagnostics.length, 0);
});

test('unreviewed loaders do not create URL aliases', async () => {
  const observer = observerFor('const syntheticLoader = 2;');
  await observer.onPause(paused);
  assert.equal(observer.cacheBusterUrls.size, 0);
  assert.equal(observer.loaderScripts.length, 0);
});

test('loader policy rejects semantic query parameters instead of stripping them', async () => {
  const observer = observerFor('const syntheticLoader = 3;', { versionProbePath: 'https://example.org/version.txt?version=2' });
  await observer.onPause(paused);
  assert.equal(observer.cacheBusterUrls.size, 0);
  assert.equal(observer.diagnostics[0].code, 'capture-error');
});

test('reviewed timestamp helper identifies exact early chart and CSV requests', async () => {
  const policy = { timestampResources: { pageQueryParam: 'id', templates: ['{id}/dos.js', '../music/artists.csv'] } };
  const observer = observerFor('const syntheticTimestampHelper = 1;', policy);
  observer.page = { url: () => 'https://example.org/work/?id=song' };
  await observer.onPause({ ...paused, callFrames: [{ callFrameId: 'helper' }] });
  assert.deepEqual([...observer.cacheBusterUrls], ['https://example.org/work/song/dos.js', 'https://example.org/music/artists.csv']);
  assert.equal(observer.scripts.length, 0);
  assert.equal(observer.loaderScripts.length, 1);
  assert.equal(observer.diagnostics.length, 0);
});

test('timestamp helper refuses semantic queries in generated resource URLs', async () => {
  const observer = observerFor('const syntheticTimestampHelper = 2;', { timestampResources: {
    pageQueryParam: 'id', templates: ['{id}/dos.js', '../music/artists.csv'],
  } });
  observer.page = { url: () => 'https://example.org/work/?id=song%3Fversion%3D2' };
  await observer.onPause({ ...paused, callFrames: [{ callFrameId: 'helper' }] });
  assert.equal(observer.cacheBusterUrls.size, 0);
  assert.equal(observer.diagnostics[0].code, 'capture-error');
});

test('early helper policy resumes real Chrome before engine execution', { timeout: 15000 }, async () => {
  const { createServer } = await import('node:http');
  const { chromium } = await import('playwright-core');
  const source = 'window.syntheticHelperExecuted = true;';
  const server = createServer((request, response) => {
    if (request.url === '/helper.js') {
      response.setHeader('content-type', 'application/javascript'); response.end(source);
    } else {
      response.setHeader('content-type', 'text/html'); response.end('<script src="/helper.js"></script>');
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}/work/?id=song`;
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage(); const observer = new EngineObserver();
  try {
    await observer.attach(page);
    observer.loaderPolicies.set(hash(source), { timestampResources: { pageQueryParam: 'id', templates: ['{id}/dos.js'] } });
    await page.goto(url, { timeout: 5000 });
    assert.equal(await page.evaluate(() => window.syntheticHelperExecuted), true);
    assert.deepEqual([...observer.cacheBusterUrls], [new URL('song/dos.js', url).href]);
    assert.equal(observer.diagnostics.length, 0);
  } finally {
    await browser.close(); await observer.close();
    await new Promise(resolve => server.close(resolve));
  }
});
