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
