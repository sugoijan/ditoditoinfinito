import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { parse } from 'acorn';
import { ENCODE_SNAPSHOT_SOURCE } from './serialization.mjs';

export const ADAPTER_ID = 'cw-lexical-cdp/1';
export const hash = data => createHash('sha256').update(data).digest('hex');

function walk(node, visit) {
  if (!node || typeof node !== 'object') return;
  if (node.type) visit(node);
  for (const [key, value] of Object.entries(node)) {
    if (key === 'loc') continue;
    if (Array.isArray(value)) value.forEach(v => walk(v, visit));
    else if (value?.type) walk(value, visit);
  }
}

// Source is parsed as syntax, never executed by Node. Only explicitly trusted
// engine source hashes may become debugger adapters.
export function findCapturePoints(source) {
  const ast = parse(source, { ecmaVersion: 'latest', locations: true });
  const points = [];
  walk(ast, node => {
    if (node.type === 'VariableDeclarator' && node.id?.type === 'Identifier') {
      const fn = node.init;
      if (!['ArrowFunctionExpression', 'FunctionExpression'].includes(fn?.type) || fn.body.type !== 'BlockStatement') return;
      const name = node.id.name;
      const params = fn.params.map(p => (p.type === 'AssignmentPattern' ? p.left : p).name);
      if (name === 'dosConvert' && params[0]) {
        points.push({ kind: 'input', location: fn.body.body[0].loc.start, params });
      }
      if (name === 'scoreConvert' && params.length === 6) {
        const last = fn.body.body.at(-1);
        if (last?.type === 'ReturnStatement' && last.argument?.type === 'Identifier') {
          points.push({ kind: 'reference', location: last.loc.start, result: last.argument.name, params });
        }
      }
      if (name === 'mainInit') points.push({ kind: 'runtime', location: fn.body.body[0].loc.start });
    }
    if (node.type === 'ExpressionStatement' && node.expression.type === 'CallExpression' &&
        node.expression.callee.name === 'pushArrows' && node.expression.arguments[0]?.name === 'g_scoreObj') {
      points.push({ kind: 'prepared', location: node.loc.start });
    }
  });
  return points;
}

function snapshotExpression(point, invocationId) {
  const reference = point.kind === 'reference';
  const [dos, scoreId, preblank, , pattern] = point.params ?? [];
  const score = reference ? point.result : 'g_scoreObj';
  return `JSON.stringify((${ENCODE_SNAPSHOT_SOURCE})((() => {
    const chartId = ${reference ? scoreId : 'g_stateObj.scoreId'};
    const keyPattern = ${reference ? pattern : '`${g_keyObj.currentKey}_${g_keyObj.currentPtn}`'};
    const sourceSuffix = String(setScoreIdHeader(chartId, g_stateObj.scoreLockFlg, false));
    const lanes = (g_keyObj['chara' + keyPattern] || []).map(chara => {
      let hold = replaceStr(chara, g_escapeStr.frzName);
      if (!hold.includes('frz') && !hold.includes('foni')) hold = 'frz' + toCapitalize(hold);
      return {chara, tapDataKey: chara + sourceSuffix + '_data', holdDataKey: hold + sourceSuffix + '_data'};
    });
    return {
      stage: ${JSON.stringify(reference ? 'scoreConvert-return-full' : point.kind)},
      chartId, invocationId: ${invocationId},
      inputDos: ${reference ? dos : 'g_rootObj'}, score: ${score},
      header: g_headerObj, state: g_stateObj, key: g_keyObj,
      gauge: typeof g_gaugeHeaderObj === 'undefined' ? undefined : g_gaugeHeaderObj[chartId],
      globals: {version: g_version, fps: g_fps, keyPattern, sourceSuffix, lanes,
        preblankFrame: ${reference ? preblank : 'null'},
        scoreNumber: setScoreIdHeader(chartId, g_stateObj.scoreLockFlg, true),
        difDataRow: typeof splitLF2 === 'function' ? (splitLF2((${reference ? dos : 'g_rootObj'}).difData)?.[chartId] ?? null) : null,
        dosId: g_headerObj.dosNos?.[chartId] ?? null,
        musicId: g_headerObj.musicNos?.[chartId] ?? 0},
      page: {url: location.href, characterSet: document.characterSet},
    };
  })()))`;
}

export class EngineObserver {
  constructor({ trustedHashes = [] } = {}) {
    this.extraHashes = trustedHashes;
    this.inputs = [];
    this.snapshots = [];
    this.scripts = [];
    this.loaderScripts = [];
    this.cacheBusterUrls = new Set();
    this.diagnostics = [];
    this.breakpoints = new Map();
    this.seenScripts = new Set();
    this.pending = Promise.resolve();
  }

  async attach(page) {
    this.page = page;
    const registry = JSON.parse(await readFile(new URL('./engine-hashes.json', import.meta.url), 'utf8'));
    this.trusted = new Set([...registry.scripts.filter(s => s.kind !== 'loader').map(s => s.sha256), ...this.extraHashes]);
    this.loaderPolicies = new Map(registry.scripts.filter(s => s.kind === 'loader').map(s => [s.sha256, s]));
    this.cdp = await page.context().newCDPSession(page);
    this.scriptUrls = new Map();
    this.cdp.on('Debugger.scriptParsed', event => this.scriptUrls.set(event.scriptId, event.url));
    await this.cdp.send('Debugger.enable');
    this.cdp.on('Debugger.paused', event => {
      this.pending = this.pending.then(() => this.onPause(event)).catch(error => {
        this.diagnostics.push({ code: 'debugger-error', message: error.message });
      });
    });
    await this.cdp.send('Debugger.setInstrumentationBreakpoint', { instrumentation: 'beforeScriptExecution' });
  }

  async onPause(event) {
    try {
      if (event.reason === 'instrumentation') {
        const frame = event.callFrames[0];
        const scriptId = event.data?.scriptId ?? frame?.location.scriptId;
        if (!scriptId || this.seenScripts.has(scriptId)) return;
        this.seenScripts.add(scriptId);
        const { scriptSource: source } = await this.cdp.send('Debugger.getScriptSource', { scriptId });
        const sha256 = hash(source);
        const url = frame?.url || this.scriptUrls?.get(scriptId) || '';
        const loaderPolicy = this.loaderPolicies?.get(sha256);
        if (loaderPolicy) {
          if (loaderPolicy.timestampResources) {
            // Use Playwright's already observed frame URL. Runtime/DOM protocol
            // evaluation during a beforeScriptExecution pause can deadlock.
            // These hash-reviewed templates use the publisher's page-relative
            // paths; a request outside those exact paths receives no alias.
            const page = { url: this.page.url(), baseURI: this.page.url() };
            const policy = loaderPolicy.timestampResources;
            const parameter = new URL(page.url).searchParams.get(policy.pageQueryParam);
            // The reviewed helper double-decodes this query parameter. Its own
            // path construction remains in Chrome; here we identify cache keys.
            const id = parameter === null ? null : decodeURIComponent(parameter.replace('+', ' '));
            const targets = [];
            for (const template of policy.templates) {
              if (template.includes('{id}') && id === null) continue;
              const target = new URL(template.replaceAll('{id}', id ?? ''), page.baseURI);
              if (!['http:', 'https:'].includes(target.protocol) || target.search || target.hash || target.username || target.password) {
                throw new Error('Reviewed timestamp resource must have an anonymous URL without query or fragment');
              }
              targets.push(target.href);
            }
            for (const target of targets) this.cacheBusterUrls.add(target);
            this.loaderScripts.push({ url, sha256, timestampResourceUrls: targets });
            return;
          }
          const probe = new URL(loaderPolicy.versionProbePath, url || loaderPolicy.url);
          if (!['https:', 'http:'].includes(probe.protocol) || probe.search || probe.hash || probe.username || probe.password) {
            throw new Error('Invalid reviewed loader version-probe policy');
          }
          this.cacheBusterUrls.add(probe.href);
          this.loaderScripts.push({ url, sha256, versionProbeUrl: probe.href });
          return;
        }
        if (!/\b(?:dosConvert|scoreConvert|mainInit)\s*=/.test(source)) return;
        if (!this.trusted.has(sha256)) {
          this.diagnostics.push({ code: 'unsupported-engine', url, sha256, message: 'Unreviewed engine source; no reference adapter installed.' });
          return;
        }
        const points = findCapturePoints(source);
        this.scripts.push({ url, sha256, adapterId: ADAPTER_ID, points: points.map(p => p.kind) });
        for (const point of points) {
          const { breakpointId } = await this.cdp.send('Debugger.setBreakpoint', {
            location: { scriptId, lineNumber: point.location.line - 1, columnNumber: point.location.column },
          });
          this.breakpoints.set(breakpointId, { ...point, url, sha256 });
        }
        return;
      }
      const point = event.hitBreakpoints?.map(id => this.breakpoints.get(id)).find(Boolean);
      if (!point) return;
      const callFrameId = event.callFrames[0].callFrameId;
      if (point.kind === 'input') {
        const result = await this.evaluate(callFrameId, `JSON.stringify({text:${point.params[0]},flags:{enableAmpersandSplit:g_enableAmpersandSplit,enableDecodeURI:g_enableDecodeURI},chartId:typeof g_stateObj==='undefined'?null:g_stateObj.scoreId,pageUrl:location.href,caller:${JSON.stringify(event.callFrames.slice(1, 5).map(f => ({ functionName: f.functionName, url: f.url, line: f.location.lineNumber + 1 })))}})`);
        this.inputs.push({ ...JSON.parse(result), sequence: this.inputs.length, engineUrl: point.url });
      } else {
        const result = await this.evaluate(callFrameId, snapshotExpression(point, this.snapshots.length));
        this.snapshots.push(JSON.parse(result));
      }
    } catch (error) {
      this.diagnostics.push({ code: 'capture-error', message: error.message });
    } finally {
      await this.cdp.send('Debugger.resume').catch(() => {});
    }
  }

  async evaluate(callFrameId, expression) {
    const response = await this.cdp.send('Debugger.evaluateOnCallFrame', { callFrameId, expression, returnByValue: true });
    if (response.exceptionDetails) throw new Error(response.exceptionDetails.text + ': ' + response.exceptionDetails.exception?.description);
    return response.result.value;
  }

  async close() {
    await this.pending;
    await this.cdp.detach().catch(() => {});
  }
}
