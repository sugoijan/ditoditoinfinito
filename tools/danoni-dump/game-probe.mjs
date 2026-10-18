// Read-only browser observations for our game; no work scripts run on the host.
export function installProbe() {
  const probe = window.__DDI_CORPUS_PROBE = { audio: [], resumes: [], starts: [], motion: [], labels: [], failures: [] };
  const OriginalContext = window.AudioContext;
  window.AudioContext = class extends OriginalContext {
    constructor(...args) {
      super(...args);
      const row = { context: this, createdActive: navigator.userActivation.isActive, analysers: [] };
      probe.audio.push(row);
      const create = this.createAnalyser.bind(this);
      this.createAnalyser = (...values) => { const a = create(...values); row.analysers.push(a); return a; };
    }
  };
  const resume = OriginalContext.prototype.resume;
  OriginalContext.prototype.resume = function (...args) {
    probe.resumes.push({ active: navigator.userActivation.isActive, state: this.state });
    return resume.apply(this, args);
  };
  const createSource = BaseAudioContext.prototype.createBufferSource;
  BaseAudioContext.prototype.createBufferSource = function (...args) {
    const node = createSource.apply(this, args), start = node.start;
    node.start = function (...values) {
      probe.starts.push({ when: values[0] ?? 0, offset: values[1] ?? 0, state: this.context.state,
        bufferSeconds: this.buffer?.duration, contextTime: this.context.currentTime });
      return start.apply(this, values);
    };
    return node;
  };
  if (window.GPUQueue) {
    const write = GPUQueue.prototype.writeBuffer;
    GPUQueue.prototype.writeBuffer = function (buffer, bufferOffset, data, dataOffset = 0, size) {
      const result = write.apply(this, arguments);
      try {
        if (buffer.label && !probe.labels.includes(buffer.label)) probe.labels.push(buffer.label);
        if (buffer.label !== 'ddi-instances' || bufferOffset !== 0 || !probe.captureMotion) return result;
        const bytes = data instanceof ArrayBuffer ? new Uint8Array(data) : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
        const unit = data instanceof ArrayBuffer ? 1 : data.BYTES_PER_ELEMENT;
        const offset = dataOffset * unit, length = size === undefined ? bytes.byteLength - offset : size * unit;
        if (length % 56) throw Error('Unexpected procedural instance stride');
        const view = new DataView(bytes.buffer, bytes.byteOffset + offset, length), glyphs = [];
        for (let at = 0; at < length; at += 56) {
          const shape = view.getUint32(at + 20, true);
          if ([1, 2, 7, 8, 16, 17, 18].includes(shape & 255)) glyphs.push({
            x: view.getFloat32(at, true), y: view.getFloat32(at + 4, true),
            width: view.getFloat32(at + 8, true), shape,
          });
        }
        probe.motion.push({ hostMs: performance.now(), debugTime: window.__DDI_DEBUG?.song_time, glyphs });
        if (probe.motion.length > 600) probe.motion.shift();
      } catch (error) { probe.failures.push(String(error)); }
      return result;
    };
  }
}

export function audioObservation() {
  const p = window.__DDI_CORPUS_PROBE;
  return { contexts: p.audio.map(row => {
    let peak = 0;
    for (const analyser of row.analysers) {
      const samples = new Float32Array(analyser.fftSize); analyser.getFloatTimeDomainData(samples);
      peak = Math.max(peak, ...samples.map(Math.abs));
    }
    return { createdActive: row.createdActive, state: row.context.state, currentTime: row.context.currentTime, peak };
  }), resumes: p.resumes, starts: p.starts };
}

export function fitMotion(frames) {
  // Follow the nearest unjudged arrow in the leftmost directional lane. Reject
  // disappearance/reuse and nonlinearity instead of mistaking a new head for motion.
  const samples = [];
  for (const frame of frames) {
    if (!Number.isFinite(frame.hostMs) || frame.glyphs.some(g => ![g.x, g.y, g.width].every(Number.isFinite) || g.width <= 0)) throw Error('Invalid submitted motion sample');
    const receptors = frame.glyphs.filter(g => g.shape === 2).sort((a, b) => a.x - b.x);
    if (!receptors.length) continue;
    const receptor = receptors[0];
    const notes = frame.glyphs.filter(g => [1, 7, 8].includes(g.shape) && Math.abs(g.x - receptor.x) < .01 && g.y > receptor.y + 5).sort((a, b) => a.y - b.y);
    if (!notes.length) continue;
    samples.push({ hostMs: frame.hostMs, y: notes[0].y, receptorY: receptor.y, arrowPixels: receptor.width / .92 });
  }
  if (samples.length < 8 || samples.at(-1).hostMs - samples[0].hostMs < 150) throw Error('Insufficient submitted-arrow observations');
  const mean = key => samples.reduce((s, p) => s + p[key], 0) / samples.length;
  const time = mean('hostMs'), y = mean('y');
  const slope = samples.reduce((s, p) => s + (p.hostMs - time) * (p.y - y), 0) / samples.reduce((s, p) => s + (p.hostMs - time) ** 2, 0);
  const residual = Math.max(...samples.map(p => Math.abs(p.y - (y + slope * (p.hostMs - time)))));
  const arrow = mean('arrowPixels');
  if (slope >= 0 || residual > 3 || samples.some(p => Math.abs(p.arrowPixels - arrow) > .01 || p.receptorY !== samples[0].receptorY)) throw Error('Nonlinear/reused arrow or unstable geometry');
  return { arrowHeightsPerSecond: -slope * 1000 / arrow, maximumPositionResidualPixels: residual, samples };
}

export async function observeUnactivatedPreview(context, url) {
  await context.addInitScript(installProbe);
  const page = await context.newPage(); await page.goto(url);
  // No locator/evaluate calls here: Playwright's evaluated functions grant
  // activation. Observe and click only via an explicitly inactive CDP call.
  const cdp = await context.newCDPSession(page);
  const response = await cdp.send('Runtime.evaluate', {
    expression: `(async () => {
      const deadline = performance.now() + 15000;
      while (!document.querySelector('.preview-button')) {
        if (performance.now() > deadline) throw Error('Preview control not ready');
        await new Promise(r => setTimeout(r, 50));
      }
      const before = { active: navigator.userActivation.isActive, ever: navigator.userActivation.hasBeenActive };
      document.querySelector('.preview-button').click();
      await new Promise(r => setTimeout(r, 800));
      return { before, observation: (${audioObservation.toString()})() };
    })()`, awaitPromise: true, returnByValue: true, userGesture: false,
  });
  if (response.exceptionDetails) throw Error('Gesture control evaluation failed');
  const value = response.result.value, { before, observation } = value;
  if (before.active || before.ever || !observation.contexts.length || observation.contexts.some(c => c.createdActive || c.state !== 'suspended' || c.currentTime !== 0 || c.peak !== 0)) throw Error('Gesture negative control did not remain suspended');
  return value;
}
