import assert from 'node:assert/strict';
import test from 'node:test';
import { fitMotion, installProbe, observeUnactivatedPreview } from '../game-probe.mjs';

const frames = (speed = 1) => Array.from({ length: 20 }, (_, i) => ({ hostMs: i * 16,
  glyphs: [{ x: 100, y: 150, width: 92, shape: 2 },
    { x: 100, y: 500 - 2.5 * speed * 100 * i * .016, width: 92, shape: 8 }] }));

test('submitted positions establish physical velocity independently of clock epoch and player multiplier', () => {
  for (const speed of [1, 2, 3]) {
    const source = frames(speed).map(f => ({ ...f, hostMs: f.hostMs + 18345 }));
    const result = fitMotion(source);
    assert.ok(Math.abs(result.arrowHeightsPerSecond - 2.5 * speed) < 1e-9);
    assert.ok(result.maximumPositionResidualPixels < 1e-9);
  }
});

test('movement proof rejects a replaced head, stale samples and changing receptor geometry', () => {
  const replaced = frames(); replaced[10].glyphs[1].y += 150;
  assert.throws(() => fitMotion(replaced), /Nonlinear/);
  assert.throws(() => fitMotion(frames().slice(0, 4)), /Insufficient/);
  const changed = frames(); changed[10].glyphs[0].width = 110;
  assert.throws(() => fitMotion(changed), /unstable geometry/);
});

test('audio/GPU observers forward the original calls and preserve submitted bytes', () => {
  const names = ['window', 'navigator', 'BaseAudioContext', 'GPUQueue'];
  const previous = new Map(names.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  const calls = [];
  class BaseContext {
    createBufferSource() { return { context: this, buffer: { duration: 123 }, start(...args) { calls.push(['start', args]); } }; }
    createAnalyser() { return {}; }
  }
  class Context extends BaseContext {
    constructor(...args) { super(); this.state = 'running'; this.currentTime = 3; calls.push(['context', args]); }
    resume(...args) { calls.push(['resume', args]); return Promise.resolve(); }
  }
  class Queue { writeBuffer(...args) { calls.push(['write', args]); return 'submitted'; } }
  try {
    for (const [name, value] of Object.entries({ window: { AudioContext: Context, GPUQueue: Queue }, navigator: { userActivation: { isActive: true } }, BaseAudioContext: BaseContext, GPUQueue: Queue })) Object.defineProperty(globalThis, name, { value, configurable: true, writable: true });
    installProbe(); const ctx = new window.AudioContext({ sampleRate: 44100 });
    ctx.resume(); const node = ctx.createBufferSource(); node.start(5, 10);
    const p = window.__DDI_CORPUS_PROBE; p.captureMotion = true;
    const bytes = new Uint8Array(56), view = new DataView(bytes.buffer);
    view.setFloat32(0, 100, true); view.setFloat32(4, 300, true); view.setFloat32(8, 92, true); view.setUint32(20, 8, true);
    const before = [...bytes], buffer = { label: 'ddi-instances' };
    assert.equal(new Queue().writeBuffer(buffer, 0, bytes), 'submitted');
    assert.deepEqual([...bytes], before);
    assert.equal(calls.at(-1)[1][0], buffer); assert.equal(calls.at(-1)[1][2], bytes);
    assert.deepEqual(calls.find(c => c[0] === 'start')[1], [5, 10]);
    assert.equal(p.audio[0].createdActive, true); assert.equal(p.starts[0].offset, 10);
    assert.equal(p.motion[0].glyphs[0].y, 300); assert.deepEqual(p.failures, []);
  } finally {
    for (const name of names) { const descriptor = previous.get(name); if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name]; }
  }
});


test('gesture negative control requests no CDP activation and rejects an already activated document', async () => {
  const value = { before: { active: false, ever: false }, observation: { contexts: [{ createdActive: false, state: 'suspended', currentTime: 0, peak: 0 }] } };
  let requests = 0;
  const page = { async goto() {}, evaluate() { throw Error('Must not grant implicit activation'); }, locator() { throw Error('Must not grant implicit activation'); } };
  const context = { async addInitScript() {}, async newPage() { return page; }, async newCDPSession() {
    return { async send(method, params) { requests++; assert.equal(method, 'Runtime.evaluate'); assert.equal(params.userGesture, false); assert.equal(params.awaitPromise, true); return { result: { value } }; } };
  } };
  assert.equal(await observeUnactivatedPreview(context, 'http://127.0.0.1:8097/'), value);
  value.before.ever = true;
  await assert.rejects(observeUnactivatedPreview(context, 'http://127.0.0.1:8097/'), /did not remain suspended/);
  assert.equal(requests, 2);
});


test('nonfinite positions or sizes cannot bypass the velocity acceptance bound', () => {
  for (const value of [NaN, Infinity, 0]) {
    const source = frames(); source[10].glyphs[0].width = value;
    assert.throws(() => fitMotion(source), /Invalid submitted motion sample/);
  }
});
