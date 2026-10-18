import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeReference as normalize, validateReference, validateManifest, validateSnapshot, validateSchema } from '../reference.mjs';
import { encodeSnapshot, decodeSnapshot } from '../serialization.mjs';

const normalizeReference = (snapshot, options = {}) => normalize(snapshot, {
  engineScriptHashes: [{ url: 'https://example.org/engine.js', sha256: 'a'.repeat(64) }],
  adapterId: 'synthetic', ...options,
});

function observed() {
  const words = [];
  words[240] = [[0, 'synthetic lyric', 30]];
  return {
    stage: 'scoreConvert-return-full', chartId: 1, invocationId: 4,
    inputDos: { difData: '5,A,3$7,B,4', adjustment: '-0.25', playbackRate: '1.25' },
    globals: { version: 'Ver 51.2.2', fps: 60, keyPattern: '7_0', sourceSuffix: '2', preblankFrame: 30,
      lanes: [{ chara: 'leftdia', tapDataKey: 'leftdia2_data', holdDataKey: 'frzLdia2_data' }] },
    score: { arrowData: [[121, 121, 122]], frzData: [[200, 240, 300]],
      dummyArrowData: [[250]], dummyFrzData: [[400, 450]], speedData: [0, 1, 120, 0, 150, -1],
      boostData: [200, 1.25], colorData: [220, 20, '#ff0000', false],
      dummyColorData: [230, 0, '#00ff00', true],
      ncolorData: { Arrow: [240, 'g0', 'red:blue', true, 'Arrow'] },
      ncolorDummyData: { Normal: [250, '0', '#fff', false, 'Normal'] }, wordData: words,
      scrollchData: [260, 0, -1, 0, ''], keychFrames: [0], keychTarget: [['0']], keychTargetAlpha: [[1]] },
    header: { blankFrame: 200, blankFrameDef: 200, playbackRate: 1.25, adjustment: ['0', '-0.25'],
      startFrame: [0, 20], keyLabels: ['5', '7'], difLabels: ['A', 'B'], creatorNames: ['Test', 'Test B'],
      difficulties: [1, 2], initSpeeds: [3, 4] },
    state: { adjustment: 0, realAdjustment: 29.8, intAdjustment: 29, decimalAdjustment: 0.8 },
    key: { currentKey: '7' }, gauge: { 1: { Border: 'x', Recovery: 6, Damage: '40', Init: 25 } },
  };
}

test('normalization keeps parsed frames, duplicate notes, odd holds, color parts, and sparse effects', () => {
  const input = observed();
  const reference = normalizeReference(encodeSnapshot(input), { adapterId: 'synthetic' });
  assert.equal(validateReference(reference).valid, true, JSON.stringify(validateReference(reference).errors));
  assert.equal(validateSnapshot(encodeSnapshot(input)).valid, true);
  assert.deepEqual(reference.lanes[0].taps, [121, 121, 122]);
  assert.deepEqual(reference.lanes[0].holdEndpoints, [200, 240, 300]);
  assert.deepEqual(reference.lanes[0].holds, [[200, 240]]);
  assert.equal(reference.diagnostics.some(x => x.code === 'odd-hold-endpoints'), true);
  assert.equal(reference.colors.modern.Arrow[0].color, 'red:blue');
  assert.equal(reference.colors.dummyModern.Normal[0].part, 'Normal');
  assert.equal(reference.timing.headerAdjustment, -0.25);
  assert.equal(reference.difData.gaugeHeader.Damage, '40');
  assert.equal(reference.difData.keyLabel, '7');
  const effects = decodeSnapshot(reference.effects);
  assert.equal(0 in effects.wordData, false);
  assert.deepEqual(effects.wordData[240], [[0, 'synthetic lyric', 30]]);
  assert.deepEqual(input.score.frzData[0], [200, 240, 300]);
});

test('older engine gauge header arrays provide exact resolved values', () => {
  const input = observed();
  delete input.gauge;
  Object.assign(input.header, { lifeBorders: ['x', '70'], lifeRecoverys: [6, '2'], lifeDamages: [40, '7'], lifeInits: [25, 50] });
  const ref = normalizeReference(encodeSnapshot(input));
  assert.deepEqual(ref.difData.gaugeHeader, { Border: '70', Recovery: '2', Damage: '7', Init: 50 });
  assert.equal(validateReference(ref).valid, true);
});

test('timer headers accept complete numeric strings while preserving native spellings', () => {
  const input = observed();
  input.header.startFrame = ['0', '0020'];
  input.header.endFrame = [' 3600 '];
  const encoded = encodeSnapshot(input);
  const ref = normalizeReference(encoded);
  assert.equal(validateReference(ref).valid, true);
  assert.equal(ref.timing.startFrame, 20);
  assert.equal(ref.timing.endFrame, 3600);
  assert.equal(decodeSnapshot(encoded).header.startFrame[1], '0020');
  for (const invalid of ['', ' ', '20frames', '20+1', 'Infinity']) {
    input.header.startFrame[1] = invalid;
    assert.equal(validateReference(normalizeReference(encodeSnapshot(input))).valid, false, invalid);
  }
});

test('analysis snapshots, nonfinite notes, missing gauges, and partial event tuples cannot pass', () => {
  for (const mutate of [
    x => { x.stage = 'analysis'; },
    x => { x.score.arrowData[0].push(NaN); },
    x => { delete x.gauge; },
    x => { x.score.speedData.push(300); },
    x => { x.globals.fps = 30; },
    x => { x.globals.lanes = []; },
  ]) {
    const input = observed(); mutate(input);
    const ref = normalizeReference(encodeSnapshot(input));
    assert.equal(validateReference(ref).valid, false);
    assert.doesNotThrow(() => JSON.stringify(ref));
  }
});

test('unrelated opaque custom function is a warning, not a failed reference', () => {
  const input = observed();
  input.key.customFunction = () => {};
  const ref = normalizeReference(encodeSnapshot(input));
  assert.equal(ref.diagnostics.some(x => x.code === 'opaque-value'), true);
  assert.equal(validateReference(ref).valid, true);
});

test('successful reference requires observed engine version, source hash, and adapter identity', () => {
  assert.equal(validateReference(normalize(encodeSnapshot(observed()))).valid, false);
  const input = observed();
  input.globals.version = null;
  assert.equal(validateReference(normalizeReference(encodeSnapshot(input))).valid, false);
});

test('manifest schema rejects traversal paths and unknown top-level fields', () => {
  const manifest = { schema: 'ddi-danoni-dump/1', page: { requestedUrl: 'https://example.org/', finalUrl: 'https://example.org/', retrievedAt: '2026-10-08T12:00:00Z' },
    engine: { name: 'danoniplus', version: null, scriptHashes: [], adapterId: null }, credits: {}, charts: [], assets: [], status: 'partial', diagnostics: [] };
  assert.equal(validateManifest(manifest).valid, true);
  assert.equal(validateManifest({ ...manifest, resourcesPath: '../outside.json' }).valid, false);
  assert.equal(validateManifest({ ...manifest, resourcesPath: '/etc/passwd' }).valid, false);
  assert.equal(validateManifest({ ...manifest, extra: 'unknown' }).valid, false);
});

test('data contracts reject lossy effective fields, undefined lane keys and invalid input flags', () => {
  const native = encodeSnapshot({ difData: '5,Test,3.5' });
  const effective = { schema: 'ddi-danoni-effective-dos/1', fields: { difData: '5,Test,3.5' }, native };
  assert.equal(validateSchema('effective-dos', effective).valid, true);
  assert.equal(validateSchema('effective-dos', { ...effective, fields: { left_data: [100] } }).valid, false);
  const keys = { schema: 'ddi-danoni-keys/1', keyMode: '5', activePattern: '5_0',
    lanes: [{ chara: 'left', tapDataKey: 'left_data', holdDataKey: 'frzLeft_data' }], definitions: encodeSnapshot({}) };
  assert.equal(validateSchema('keys', keys).valid, true);
  keys.lanes[0].holdDataKey = undefined;
  assert.equal(validateSchema('keys', keys).valid, false);
  const inputs = { schema: 'ddi-danoni-inputs/1', inputs: [{ sequence: 0, chartId: null,
    pageUrl: 'https://example.org/', engineUrl: 'https://example.org/js/main.js', caller: [],
    flags: { enableAmpersandSplit: true, enableDecodeURI: false }, textPath: 'inputs/0000.dos.txt', stringPath: 'inputs/0000.string.json' }] };
  assert.equal(validateSchema('inputs', inputs).valid, true);
  inputs.inputs[0].flags.enableDecodeURI = 'false';
  assert.equal(validateSchema('inputs', inputs).valid, false);
});
