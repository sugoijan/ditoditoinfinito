import { readFileSync } from 'node:fs';
import Ajv from 'ajv';
import { decodeSnapshot, encodeSnapshot, projectSnapshot } from './serialization.mjs';

const schemas = Object.fromEntries(['snapshot', 'reference', 'manifest', 'effective-dos', 'keys', 'inputs'].map(name =>
  [name, JSON.parse(readFileSync(new URL(`./schemas/${name}.schema.json`, import.meta.url), 'utf8'))]));
const ajv = new Ajv({ allErrors: true, strict: true, allowUnionTypes: true });
for (const schema of Object.values(schemas)) ajv.addSchema(schema);

export function validateSchema(name, value) {
  if (!schemas[name]) throw new TypeError(`Unknown schema: ${name}`);
  const validator = ajv.getSchema(schemas[name].$id);
  const valid = validator(value);
  return { valid, errors: structuredClone(validator.errors ?? []) };
}

export function validateReference(value) {
  const result = validateSchema('reference', value);
  const errors = (value?.diagnostics ?? []).filter(item => item.severity === 'error')
    .map(item => ({ instancePath: item.path, keyword: 'capture', message: item.message, params: { code: item.code } }));
  return { valid: result.valid && errors.length === 0, errors: [...result.errors, ...errors] };
}

export const validateManifest = value => validateSchema('manifest', value);
export const validateSnapshot = value => validateSchema('snapshot', value);

/** Project an observed engine return, without parsing dos or executing page code. */
export function normalizeReference(encoded, { engineScriptHashes = [], adapterId = 'unknown', snapshotPath = 'engine-snapshots.json' } = {}) {
  const isEncoded = encoded?.schema === 'ddi-danoni-snapshot/1';
  const snapshot = isEncoded ? decodeSnapshot(encoded) : encoded;
  const { score = {}, header = {}, state = {}, key = {}, globals = {}, inputDos = {} } = snapshot;
  const chartId = snapshot.chartId;
  const diagnostics = (isEncoded ? encoded.diagnostics : []).map(item => ({ ...item, severity: 'warning' }));
  const issue = (code, path, message, severity = 'error') => diagnostics.push({ severity, code, path, message });
  const number = (value, path, optional = false) => {
    if (optional && (value === undefined || value === null)) return null;
    if (typeof value !== 'number' || !Number.isFinite(value)) {
      issue('invalid-number', path, 'Expected a finite engine number');
      return null;
    }
    return value;
  };
  const scalar = (value, path) => {
    if (value === undefined || value === null) return null;
    if (typeof value === 'string' || typeof value === 'boolean') return value;
    if (typeof value === 'number') return number(value, path);
    issue('invalid-scalar', path, 'Expected a plain engine scalar');
    return null;
  };
  const numericHeader = (value, path) => value === undefined || value === null ? null
    : number(typeof value === 'string' ? Number.parseFloat(value) : value, path);
  // transTimerToFrame keeps non-timer inputs as strings. Convert only complete
  // numeric spellings; this is a representation change, not another dos parser.
  const frameHeader = (value, path) => value === undefined || value === null ? null
    : number(typeof value === 'string' && value.trim() !== '' ? Number(value) : value, path);
  const numericArray = (value, path, optional = false) => {
    if (optional && value === undefined) return [];
    if (!Array.isArray(value)) {
      issue('missing-array', path, 'Expected an engine number array');
      return [];
    }
    return Array.from(value, (item, index) => number(item, `${path}/${index}`));
  };
  const groups = (value, stride, path, convert, optional = false) => {
    if (optional && value === undefined) return [];
    if (!Array.isArray(value)) {
      issue('missing-array', path, 'Expected a flat engine event array');
      return [];
    }
    if (value.length % stride !== 0) issue('incomplete-event', path, `Length must be a multiple of ${stride}; full native data remains in the snapshot`);
    const out = [];
    for (let i = 0; i + stride <= value.length; i += stride) out.push(convert(value.slice(i, i + stride), `${path}/${i}`));
    return out;
  };
  const text = (value, path) => {
    if (typeof value === 'string') return value;
    issue('invalid-string', path, 'Expected an engine string');
    return null;
  };
  const boolean = (value, path) => {
    if (typeof value === 'boolean') return value;
    issue('invalid-boolean', path, 'Expected an engine boolean');
    return null;
  };
  const speedEvents = prop => groups(score[prop], 2, `/score/${prop}`, ([frame, multiplier], path) =>
    ({ frame: number(frame, `${path}/frame`), multiplier: number(multiplier, `${path}/multiplier`) }));
  const legacyColors = (prop, optional = false) => groups(score[prop], 4, `/score/${prop}`, ([frame, target, color, all], path) => ({
    frame: number(frame, `${path}/frame`), target: number(target, `${path}/target`),
    color: text(color, `${path}/color`), all: boolean(all, `${path}/all`),
  }), optional);
  const modernColors = (prop, optional = false) => {
    const value = score[prop];
    if (value === undefined && optional) return {};
    if (!value || typeof value !== 'object' || Array.isArray(value) || value.$opaque) {
      issue('missing-color-map', `/score/${prop}`, 'Expected engine color-part map');
      return {};
    }
    return Object.fromEntries(Object.entries(value).map(([part, data]) => [part,
      groups(data, 5, `/score/${prop}/${part}`, ([frame, target, color, all, nativePart], path) => ({
        frame: number(frame, `${path}/frame`), target: text(target, `${path}/target`),
        color: text(color, `${path}/color`), all: boolean(all, `${path}/all`), part: text(nativePart, `${path}/part`),
      }))]));
  };

  if (snapshot.stage !== 'scoreConvert-return-full') issue('wrong-stage', '/captureStage', 'Reference requires the full converter return before play preparation');
  if (typeof globals.version !== 'string' || !globals.version.trim()) issue('missing-engine-version', '/engineVersion', 'Observed engine version is required');
  if (!Array.isArray(engineScriptHashes) || engineScriptHashes.length === 0) issue('missing-engine-hashes', '/engineScriptHashes', 'At least one observed engine script hash is required');
  if (typeof adapterId !== 'string' || !adapterId.trim() || adapterId === 'unknown') issue('missing-adapter', '/adapterId', 'A reviewed adapter identity is required');
  if (globals.fps !== 60) issue('unsupported-fps', '/fps', 'The initial adapter requires observed g_fps = 60');
  let laneInfo = globals.lanes;
  if (!Array.isArray(laneInfo)) {
    issue('missing-lane-definitions', '/lanes', 'Adapter must capture resolved lane data keys');
    laneInfo = [];
  }
  if (laneInfo.length === 0) issue('empty-lanes', '/lanes', 'No resolved lanes were captured');
  for (const prop of ['arrowData', 'frzData']) {
    if (!Array.isArray(score[prop]) || score[prop].length !== laneInfo.length) {
      issue('lane-count-mismatch', `/score/${prop}`, 'Engine lane arrays do not match resolved lane definitions');
    }
  }
  const lanes = laneInfo.map((lane, index) => {
    const holdEndpoints = numericArray(score.frzData?.[index], `/lanes/${index}/holdEndpoints`);
    const dummyHoldEndpoints = numericArray(score.dummyFrzData?.[index], `/lanes/${index}/dummyHoldEndpoints`, true);
    for (const [name, endpoints] of [['holdEndpoints', holdEndpoints], ['dummyHoldEndpoints', dummyHoldEndpoints]]) {
      if (endpoints.length % 2) issue('odd-hold-endpoints', `/lanes/${index}/${name}`, 'Unmatched final hold endpoint retained', 'warning');
    }
    const holds = [];
    for (let i = 0; i + 1 < holdEndpoints.length; i += 2) holds.push(holdEndpoints.slice(i, i + 2));
    return {
      index, chara: text(lane.chara, `/lanes/${index}/chara`),
      tapDataKey: text(lane.tapDataKey, `/lanes/${index}/tapDataKey`),
      holdDataKey: text(lane.holdDataKey, `/lanes/${index}/holdDataKey`),
      taps: numericArray(score.arrowData?.[index], `/lanes/${index}/taps`), holdEndpoints, holds,
      dummyTaps: numericArray(score.dummyArrowData?.[index], `/lanes/${index}/dummyTaps`, true), dummyHoldEndpoints,
    };
  });

  const perChart = (name, fallback = false) => header[name]?.[chartId] ?? (fallback ? header[name]?.[0] : undefined);
  const gauge = snapshot.gauge?.[chartId] ?? (snapshot.gauge?.Border !== undefined ? snapshot.gauge : undefined) ?? {
    Border: perChart('lifeBorders'), Recovery: perChart('lifeRecoverys'),
    Damage: perChart('lifeDamages'), Init: perChart('lifeInits'),
  };
  const gaugeHeader = Object.fromEntries(['Border', 'Recovery', 'Damage', 'Init'].map(name => {
    if (gauge[name] === undefined) issue('missing-gauge', `/difData/gaugeHeader/${name}`, 'Resolved gauge header is missing');
    return [name, scalar(gauge[name], `/difData/gaugeHeader/${name}`)];
  }));
  const direct = ['arrowData', 'frzData', 'dummyArrowData', 'dummyFrzData', 'speedData', 'boostData',
    'colorData', 'dummyColorData', 'ncolorData', 'ncolorDummyData'];
  const effects = isEncoded ? projectSnapshot(encoded, ['score'], { exclude: direct })
    : encodeSnapshot(Object.fromEntries(Object.entries(score).filter(([name]) => !direct.includes(name))));
  const rawTiming = Object.fromEntries(['blankFrame', 'startFrame', 'endFrame', 'fadeFrame', 'adjustment', 'playbackRate']
    .map(name => [name, scalar(inputDos[name], `/timing/raw/${name}`)]));
  const reference = {
    schema: 'ddi-danoni-reference/1', chartId, patternId: globals.keyPattern,
    engineVersion: globals.version ?? null, engineScriptHashes, adapterId,
    captureStage: snapshot.stage, invocationId: snapshot.invocationId,
    fps: globals.fps,
    timing: {
      raw: rawTiming, blankFrame: number(header.blankFrame, '/timing/blankFrame', true),
      blankFrameDef: number(header.blankFrameDef, '/timing/blankFrameDef', true),
      startFrame: frameHeader(perChart('startFrame', true), '/timing/startFrame'),
      endFrame: frameHeader(perChart('endFrame', true), '/timing/endFrame'),
      playbackRate: number(header.playbackRate, '/timing/playbackRate'),
      headerAdjustment: numericHeader(perChart('adjustment', true), '/timing/headerAdjustment'),
      userAdjustment: numericHeader(state.adjustment, '/timing/userAdjustment'),
      preblankFrame: number(globals.preblankFrame, '/timing/preblankFrame', true),
      realAdjustment: number(state.realAdjustment, '/timing/realAdjustment', true),
      intAdjustment: number(state.intAdjustment, '/timing/intAdjustment', true),
      decimalAdjustment: number(state.decimalAdjustment, '/timing/decimalAdjustment', true),
      runtimeAddedPadding: number(globals.runtimeAddedPadding, '/timing/runtimeAddedPadding', true),
    },
    difData: {
      raw: scalar(inputDos.difData, '/difData/raw'), rawRow: scalar(globals.difDataRow, '/difData/rawRow'),
      keyLabel: scalar(perChart('keyLabels') ?? key.currentKey, '/difData/keyLabel'),
      label: scalar(perChart('difLabels'), '/difData/label'), creator: scalar(perChart('creatorNames'), '/difData/creator'),
      difficulty: number(perChart('difficulties'), '/difData/difficulty', true),
      initialSpeed: number(perChart('initSpeeds'), '/difData/initialSpeed', true), gaugeHeader,
    },
    lanes, speed: speedEvents('speedData'), boost: speedEvents('boostData'),
    colors: { legacy: legacyColors('colorData'), modern: modernColors('ncolorData', true),
      dummyLegacy: legacyColors('dummyColorData', true), dummyModern: modernColors('ncolorDummyData', true) },
    effects, snapshotPath, diagnostics,
  };
  return reference;
}
