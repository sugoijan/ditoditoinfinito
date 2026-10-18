/** Serialize observed browser values without invoking getters or toJSON methods.
 * Self-contained by design: the function source is injected into CDP call frames.
 * Plain data values are lossless; accessors, symbols, functions and exotic objects
 * are opaque and diagnosed. Proxy traps cannot be identified without invoking them.
 */
export function encodeSnapshot(value) {
  const seen = new Map();
  const diagnostics = [];
  const opaque = (kind, path) => {
    diagnostics.push({ code: 'opaque-value', path, message: `Cannot capture ${kind} as plain data` });
    return { $type: 'opaque', kind };
  };
  const visit = (item, path) => {
    if (item === null || typeof item === 'string' || typeof item === 'boolean') return item;
    if (typeof item === 'undefined') return { $type: 'undefined' };
    if (typeof item === 'number') {
      if (Number.isNaN(item)) return { $type: 'number', value: 'NaN' };
      if (!Number.isFinite(item)) return { $type: 'number', value: item > 0 ? 'Infinity' : '-Infinity' };
      if (Object.is(item, -0)) return { $type: 'number', value: '-0' };
      return item;
    }
    if (typeof item === 'bigint') return { $type: 'bigint', value: String(item) };
    if (typeof item === 'function' || typeof item === 'symbol') return opaque(typeof item, path);
    if (seen.has(item)) return { $type: 'ref', id: seen.get(item) };
    const array = Array.isArray(item);
    let proto, descriptors;
    try {
      proto = Object.getPrototypeOf(item);
      if (!array && proto !== Object.prototype && proto !== null) return opaque('non-plain-object', path);
      descriptors = Object.getOwnPropertyDescriptors(item);
    } catch {
      return opaque('uninspectable-object', path);
    }
    const id = seen.size;
    seen.set(item, id);
    const result = { $type: array ? 'array' : 'object', id, entries: [] };
    if (array) result.length = descriptors.length.value;
    else result.nullPrototype = proto === null;
    for (const key of Reflect.ownKeys(descriptors)) {
      if (array && key === 'length') continue;
      if (typeof key === 'symbol') {
        opaque('symbol-key', path);
        continue;
      }
      const descriptor = descriptors[key];
      const nextPath = `${path}[${JSON.stringify(key)}]`;
      const encoded = Object.prototype.hasOwnProperty.call(descriptor, 'value')
        ? visit(descriptor.value, nextPath) : opaque('accessor', nextPath);
      result.entries.push([key, encoded]);
    }
    return result;
  };
  return { schema: 'ddi-danoni-snapshot/1', root: visit(value, '$'), diagnostics };
}

export const ENCODE_SNAPSHOT_SOURCE = `(${encodeSnapshot.toString()})`;

/** Decode data only. No function, getter, class, or downloaded source is executed. */
export function decodeSnapshot(snapshot) {
  if (snapshot?.schema !== 'ddi-danoni-snapshot/1') throw new TypeError('Unknown snapshot encoding');
  const refs = new Map();
  const visit = node => {
    if (node === null || typeof node !== 'object') return node;
    switch (node.$type) {
      case 'undefined': return undefined;
      case 'number': {
        const numbers = { NaN, Infinity, '-Infinity': -Infinity, '-0': -0 };
        if (!Object.hasOwn(numbers, node.value)) throw new TypeError('Invalid tagged number');
        return numbers[node.value];
      }
      case 'bigint': return BigInt(node.value);
      case 'opaque': return Object.freeze({ $opaque: node.kind });
      case 'ref':
        if (!refs.has(node.id)) throw new TypeError('Forward or missing snapshot reference');
        return refs.get(node.id);
      case 'array':
      case 'object': {
        if (!Number.isSafeInteger(node.id) || node.id < 0 || refs.has(node.id)) throw new TypeError('Invalid snapshot object id');
        if (node.$type === 'array' && (!Number.isSafeInteger(node.length) || node.length < 0 || node.length > 0xffffffff)) {
          throw new TypeError('Invalid array length');
        }
        const out = node.$type === 'array' ? new Array(node.length)
          : Object.create(node.nullPrototype ? null : Object.prototype);
        refs.set(node.id, out);
        const keys = new Set();
        if (!Array.isArray(node.entries)) throw new TypeError('Invalid snapshot entries');
        for (const entry of node.entries) {
          if (!Array.isArray(entry) || entry.length !== 2 || typeof entry[0] !== 'string' || keys.has(entry[0])) {
            throw new TypeError('Invalid snapshot property');
          }
          const [key, value] = entry;
          if (node.$type === 'array' && key === 'length') throw new TypeError('Cannot overwrite array length');
          keys.add(key);
          Object.defineProperty(out, key, { value: visit(value), enumerable: true, configurable: true, writable: true });
        }
        return out;
      }
      default: throw new TypeError('Unknown snapshot value tag');
    }
  };
  return visit(snapshot.root);
}

/** Select a captured subtree without decoding/re-encoding opaque browser values.
 * References to nodes outside that subtree are included once and renumbered.
 */
export function projectSnapshot(snapshot, propertyPath, { exclude = [] } = {}) {
  if (snapshot?.schema !== 'ddi-danoni-snapshot/1' || !Array.isArray(propertyPath) || propertyPath.some(key => typeof key !== 'string')) {
    throw new TypeError('Expected a snapshot and a string property path');
  }
  const nodes = new Map();
  const collect = node => {
    if (node?.$type !== 'object' && node?.$type !== 'array') return;
    if (nodes.has(node.id)) throw new TypeError('Duplicate snapshot object id');
    nodes.set(node.id, node);
    for (const [, child] of node.entries) collect(child);
  };
  collect(snapshot.root);
  const resolve = node => {
    if (node?.$type !== 'ref') return node;
    if (!nodes.has(node.id)) throw new TypeError('Missing snapshot reference');
    return nodes.get(node.id);
  };
  let selected = snapshot.root;
  for (const key of propertyPath) {
    selected = resolve(selected);
    if (selected?.$type !== 'object' && selected?.$type !== 'array') throw new TypeError('Snapshot path is not an object');
    const entry = selected.entries.find(([name]) => name === key);
    if (!entry) throw new TypeError(`Missing snapshot property: ${key}`);
    selected = entry[1];
  }
  if (exclude.length) {
    selected = resolve(selected);
    if (selected?.$type !== 'object' && selected?.$type !== 'array') throw new TypeError('Cannot select fields from a scalar snapshot');
    selected = { ...selected, entries: selected.entries.filter(([name]) => !exclude.includes(name)) };
  }
  const emitted = new Map();
  const clone = node => {
    node = resolve(node);
    if (node?.$type !== 'object' && node?.$type !== 'array') return structuredClone(node);
    if (emitted.has(node.id)) return { $type: 'ref', id: emitted.get(node.id) };
    const id = emitted.size;
    emitted.set(node.id, id);
    return { ...node, id, entries: node.entries.map(([name, child]) => [name, clone(child)]) };
  };
  // Keep original diagnostic paths: a selected alias may refer to a value first
  // encountered outside its own path. Filtering by prefix would hide that fact.
  return { schema: 'ddi-danoni-snapshot/1', root: clone(selected), diagnostics: structuredClone(snapshot.diagnostics ?? []) };
}
