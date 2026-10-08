import test from 'node:test';
import assert from 'node:assert/strict';
import { decodeSnapshot, encodeSnapshot, projectSnapshot, ENCODE_SNAPSHOT_SOURCE } from '../serialization.mjs';

test('snapshot preserves sparse arrays, nonfinite values, aliases, and cycles through JSON', () => {
  const sparse = new Array(1000);
  sparse[5] = undefined;
  sparse[9] = NaN;
  sparse[11] = Infinity;
  sparse[12] = -Infinity;
  sparse[13] = -0;
  sparse[14] = 12345678901234567890n;
  const input = { sparse, alias: sparse, collision: { $type: 'ref', id: 100 } };
  input.self = input;
  const encoded = JSON.parse(JSON.stringify(encodeSnapshot(input)));
  assert.ok(JSON.stringify(encoded).length < 1400, 'holes must not expand into nulls');
  const out = decodeSnapshot(encoded);
  assert.equal(out.self, out);
  assert.equal(out.alias, out.sparse);
  assert.equal(out.sparse.length, 1000);
  assert.equal(0 in out.sparse, false);
  assert.equal(5 in out.sparse, true);
  assert.equal(out.sparse[5], undefined);
  assert.equal(Number.isNaN(out.sparse[9]), true);
  assert.equal(out.sparse[11], Infinity);
  assert.equal(out.sparse[12], -Infinity);
  assert.equal(Object.is(out.sparse[13], -0), true);
  assert.equal(out.sparse[14], 12345678901234567890n);
  assert.deepEqual(out.collision, { $type: 'ref', id: 100 });
});

test('snapshot does not call accessors, toJSON, or downloaded functions', () => {
  let calls = 0;
  const input = { toJSON() { calls++; throw Error('must not run'); }, fn() { calls++; } };
  Object.defineProperty(input, 'secret', { enumerable: true, get() { calls++; throw Error('must not run'); } });
  const encoded = encodeSnapshot(input);
  JSON.stringify(encoded);
  assert.equal(calls, 0);
  assert.equal(encoded.diagnostics.length, 3);
  assert.deepEqual(decodeSnapshot(encoded).secret, { $opaque: 'accessor' });
});

test('null prototypes and __proto__ fields do not pollute host objects', () => {
  const input = Object.create(null);
  input.__proto__ = { polluted: true };
  input.constructor = 'chart-data';
  const out = decodeSnapshot(encodeSnapshot(input));
  assert.equal(Object.getPrototypeOf(out), null);
  assert.equal(out.__proto__.polluted, true);
  assert.equal({}.polluted, undefined);
});

test('unsupported objects and symbols are diagnosed instead of silently becoming empty objects', () => {
  const encoded = encodeSnapshot({ when: new Date(), map: new Map(), symbol: Symbol('x') });
  assert.equal(encoded.diagnostics.length, 3);
  assert.equal(decodeSnapshot(encoded).when.$opaque, 'non-plain-object');
});

test('standalone encoder source references no module helpers', () => {
  // Only our own function source is evaluated; downloaded scripts are never host-evaluated.
  const encoder = Function(`return ${ENCODE_SNAPSHOT_SOURCE}`)();
  assert.deepEqual(encoder({ value: undefined }), encodeSnapshot({ value: undefined }));
});

test('invalid references, lengths and duplicate properties are rejected', () => {
  const envelope = root => ({ schema: 'ddi-danoni-snapshot/1', root, diagnostics: [] });
  assert.throws(() => decodeSnapshot(envelope({ $type: 'ref', id: 99 })), /missing/);
  assert.throws(() => decodeSnapshot(envelope({ $type: 'array', id: 0, length: -1, entries: [] })), /length/);
  assert.throws(() => decodeSnapshot(envelope({ $type: 'object', id: 0, entries: [['x', 1], ['x', 2]] })), /property/);
});

test('JSON snapshot preserves lone UTF-16 surrogates and exact source whitespace', () => {
  const source = '|word_data=100,0,\ud800|\r\n|title=  test  |';
  const out = decodeSnapshot(JSON.parse(JSON.stringify(encodeSnapshot(source))));
  assert.equal(out, source);
});

test('subtree projection preserves opaque tags and aliases defined outside the subtree', () => {
  let calls = 0;
  const shared = { frames: [120, 240] };
  const input = { shared, key: { alias: shared, custom() {} } };
  Object.defineProperty(input.key, 'getter', { enumerable: true, get() { calls++; } });
  input.key.self = input.key;
  const projected = projectSnapshot(encodeSnapshot(input), ['key']);
  const custom = projected.root.entries.find(([key]) => key === 'custom')[1];
  assert.equal(custom.$type, 'opaque');
  assert.equal(custom.kind, 'function');
  const decoded = decodeSnapshot(projected);
  assert.equal(decoded.self, decoded);
  assert.deepEqual(decoded.alias.frames, [120, 240]);
  assert.equal(calls, 0);
  assert.throws(() => projectSnapshot(encodeSnapshot(input), ['missing']), /Missing/);
  const excluded = decodeSnapshot(projectSnapshot(encodeSnapshot(input), ['key'], { exclude: ['custom', 'getter'] }));
  assert.equal('custom' in excluded, false);
  assert.equal(excluded.self, excluded);
  assert.deepEqual(excluded.alias.frames, [120, 240]);
});
