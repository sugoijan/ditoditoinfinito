/* Local design study; no game, network, build step, or third-party runtime.
 * The texture math follows title-design/preview.js and Infinito's WGSL;
 * coordinates, green mapping and timings here are exploratory HUD choices. */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const clamp = (v, a = 0, b = 1) => Math.max(a, Math.min(b, v));
  const mix = (a, b, t) => a + (b - a) * t;
  const fract = v => v - Math.floor(v);
  const ease = t => 1 - (1 - clamp(t)) ** 3;
  const rgb = hex => [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16));
  const palette = window.DDI_TITLE.palette.map(rgb);
  const storageKey = 'ddi.play-hud-study.v1';
  const systemMotion = matchMedia('(prefers-reduced-motion: reduce)');

  // One table supplies controls, defaults and import validation.
  const ranges = {
    width: ['Bar width in context', 'shape', 42, 20, 65, 1, '%'],
    height: ['Bar height', 'shape', 16, 6, 32, 1, 'px'],
    radius: ['Corner radius', 'shape', 5, 0, 16, 1, 'px'],
    low: ['Low-life threshold', 'shape', 25, 5, 40, 1, '%'],
    contrast: ['Texture contrast', 'motion', .24, 0, .8, .01, ''],
    scale: ['Texture scale', 'motion', .018, .004, .06, .001, ''],
    speed: ['Normal turbulence', 'motion', .22, 0, 1.5, .01, '×'],
    fullSpeed: ['Full-life turbulence', 'motion', .75, 0, 3, .05, '×'],
    paletteSeconds: ['Full-life palette cycle', 'motion', 8, 2, 20, .5, 's'],
    delay: ['Damage hold', 'motion', 80, 0, 400, 10, 'ms'],
    loss: ['Damage contraction', 'motion', 380, 100, 1000, 10, 'ms'],
    gain: ['Recovery transition', 'motion', 130, 0, 500, 10, 'ms'],
    fullBlend: ['Full-life color arrival', 'motion', 240, 0, 800, 10, 'ms'],
    judgmentSize: ['Judgment size', 'type', 38, 22, 54, 1, 'px'],
    comboSize: ['Combo size', 'type', 64, 32, 78, 1, 'px'],
    scoreSize: ['Score size', 'type', 32, 20, 46, 1, 'px'],
    numberWeight: ['Number weight', 'type', 600, 400, 800, 100, ''],
    tierStrength: ['Combo outline strength', 'type', 1, 0, 1.5, .05, '×'],
    comboGap: ['Combo outline gap', 'type', 2.5, 0, 6, .25, 'px'],
    thousandScale: ['1000 size multiplier', 'type', 1.18, 1, 1.4, .01, '×'],
    thousandOutline: ['1000 outline multiplier', 'type', 1.2, .5, 2, .05, '×'],
    markOpacity: ['Opacity', 'watermark', .16, 0, .4, .01, ''],
    markWidth: ['Width', 'watermark', 160, 80, 240, 5, 'px'],
  };
  const colors = {
    jade: ['Deep jade', '#348d70'], mint: ['Fresh mint', '#9cddb1'],
    damageDeep: ['Damage · deep', '#823e49'], damage: ['Damage · light', '#d96b68'],
    recoveryDeep: ['Recovery · deep', '#285b9b'], recoveryLight: ['Recovery · light', '#83c9f4'],
    warning: ['Low life', '#df695e'], track: ['Track', '#24212b'], markOutline: ['Initials outline', '#a39c95'],
  };
  const defaults = {
    ...Object.fromEntries(Object.entries(ranges).map(([k, v]) => [k, v[2]])),
    ...Object.fromEntries(Object.entries(colors).map(([k, v]) => [k, v[1]])),
    font: 'iosevka', word: 'Fantastic!', scoreFormat: 'percent', combo: 300,
    mark: true, still: false, reduced: systemMotion.matches, background: 'quiet', thousandStyle: 'inverted',
  };
  const choices = { font: ['dosis', 'iosevka'], word: [...$('word').options].map(o => o.value), scoreFormat: ['percent', 'money'], background: ['quiet', 'bright'], thousandStyle: ['inverted', 'classic'] };
  let settings = { ...defaults };
  function validate(source) {
    if (!source || typeof source !== 'object' || Array.isArray(source)) throw new Error('Expected a settings object.');
    const next = { ...defaults };
    for (const [key, [, , , min, max]] of Object.entries(ranges)) {
      if (Number.isFinite(source[key])) next[key] = clamp(source[key], min, max);
    }
    for (const key of Object.keys(colors)) if (/^#[a-f0-9]{6}$/i.test(source[key])) next[key] = source[key];
    for (const [key, values] of Object.entries(choices)) if (values.includes(source[key])) next[key] = source[key];
    for (const key of ['mark', 'still', 'reduced']) if (typeof source[key] === 'boolean') next[key] = source[key];
    if (Number.isFinite(source.combo)) next.combo = Math.round(clamp(source.combo, 0, 99999));
    return next;
  }
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey));
    if (saved?.version === 1) settings = validate(saved.settings);
  } catch { /* Private browsing, missing or old state: use defaults. */ }
  // Respect the OS setting even if this study was saved on another display.
  if (systemMotion.matches) settings.reduced = true;
  function save() {
    try { localStorage.setItem(storageKey, JSON.stringify({ version: 1, settings })); }
    catch { $('status').textContent = 'Browser storage is unavailable. Export settings to keep these values.'; }
  }

  let clock = 0, previous = performance.now(), lastPaint = -Infinity;
  let life = 78, fill = 78, trail = 78, trailAlpha = 0, fullMix = 0;
  let recovery = null, damage = null, demoStart = null, demoIndex = 0;
  let evolution = 4, phase = .626, comboPulse = -Infinity, dirty = true;
  const bars = [...document.querySelectorAll('.life')];
  const fieldCanvas = $('field'), comboCanvas = $('combo');
  const texture = document.createElement('canvas');
  texture.width = 520; texture.height = 32;
  const textureContext = texture.getContext('2d');
  const pixels = textureContext.createImageData(texture.width, texture.height);
  const accents = [
    { low: 'damageDeep', high: 'damage' },
    { low: 'recoveryDeep', high: 'recoveryLight' },
  ].map(keys => {
    const canvas = document.createElement('canvas');
    canvas.width = texture.width; canvas.height = texture.height;
    const context = canvas.getContext('2d');
    return { ...keys, canvas, context, pixels: context.createImageData(canvas.width, canvas.height) };
  });
  const comboLayer = document.createElement('canvas');
  const normal = { jade: rgb(settings.jade), mint: rgb(settings.mint) };

  for (const [key, [label, group, , min, max, step, unit]] of Object.entries(ranges)) {
    const wrapper = document.createElement('div');
    const caption = document.createElement('label');
    caption.htmlFor = key; caption.append(label);
    const output = document.createElement('output'); output.id = `${key}-value`; caption.append(output);
    const input = document.createElement('input');
    Object.assign(input, { type: 'range', id: key, min, max, step });
    wrapper.append(caption, input); $(`${group}-controls`).append(wrapper);
    input.addEventListener('input', () => { settings[key] = Number(input.value); sync(); save(); });
    output.dataset.unit = unit;
  }
  for (const [key, [label]] of Object.entries(colors)) {
    const wrapper = document.createElement('label'); wrapper.append(label);
    const input = document.createElement('input'); Object.assign(input, { type: 'color', id: `color-${key}` });
    wrapper.append(input); $(key === 'markOutline' ? 'watermark-controls' : 'shape-controls').append(wrapper);
    input.addEventListener('input', () => { settings[key] = input.value; sync(); save(); });
  }
  const wordColors = {
    'Fantastic!': '#8cf2ff', 'Marvelous!!!': '#8cf2ff', 'Excellent!': '#ffe659',
    'Perfect!!': '#ffe659', Great: '#73f273', Decent: '#bf80ff', 'Way Off': '#ff8c4d', Miss: '#ff4d4d',
  };
  const bindings = { font: 'font', word: 'word', 'score-format': 'scoreFormat', background: 'background', 'show-mark': 'mark', still: 'still', reduced: 'reduced', 'thousand-style': 'thousandStyle' };
  for (const [id, key] of Object.entries(bindings)) {
    $(id).addEventListener('change', () => {
      settings[key] = $(id).type === 'checkbox' ? $(id).checked : $(id).value;
      if (key === 'reduced') { recovery = null; damage = null; fill = trail = life; trailAlpha = 0; }
      sync(); save();
    });
  }
  function sync() {
    for (const [key, [, , , , , , unit]] of Object.entries(ranges)) {
      $(key).value = settings[key]; $(`${key}-value`).textContent = `${settings[key]}${unit}`;
    }
    for (const key of Object.keys(colors)) $(`color-${key}`).value = settings[key];
    for (const [id, key] of Object.entries(bindings)) {
      if ($(id).type === 'checkbox') $(id).checked = settings[key]; else $(id).value = settings[key];
    }
    normal.jade = rgb(settings.jade); normal.mint = rgb(settings.mint);
    for (const accent of accents) { accent.from = rgb(settings[accent.low]); accent.to = rgb(settings[accent.high]); }
    document.querySelector('.stage-life').style.width = `${settings.width}%`;
    $('stage').classList.toggle('bright', settings.background === 'bright');
    const family = settings.font === 'dosis' ? 'Dosis' : 'IosevkaHUD';
    $('judgment').style.fontFamily = family;
    $('judgment').style.fontSize = `${settings.judgmentSize}px`;
    $('judgment').style.color = wordColors[settings.word]; $('judgment').textContent = settings.word;
    document.querySelector('.score').style.fontFamily = family;
    document.querySelector('.score').style.fontSize = `${settings.scoreSize}px`;
    document.querySelector('.score').style.fontWeight = settings.numberWeight;
    $('score').textContent = settings.scoreFormat === 'money' ? '987650' : '98.76%';
    $('watermark').style.opacity = settings.mark ? settings.markOpacity : 0;
    $('watermark').style.width = `${settings.markWidth}px`;
    $('mark-rings').setAttribute('stroke', settings.markOutline);
    $('combo-value').value = settings.combo;
    $('tier-status').textContent = settings.combo >= 1000 ? `1000+ · ${settings.thousandStyle === 'inverted' ? 'color text / white outline' : 'white text / color outline'} · gap` : settings.combo >= 100 ? `Tier ${Math.floor(settings.combo / 100) * 100} · white text / color outline · gap` : 'Base · warm white';
    comboCanvas.setAttribute('aria-label', `Combo ${settings.combo}`);
    dirty = true;
  }
  const title = window.DDI_TITLE.title;
  // Same 12-unit outline and 12-unit clearance as the approved title.
  // Mask every glyph, so the D/D/I rings cannot run into neighboring letters.
  const svg = (tag, attrs) => {
    const node = document.createElementNS('http://www.w3.org/2000/svg', tag);
    for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
    return node;
  };
  const markPad = 26;
  const markBounds = { x: -markPad, y: -markPad, width: title.width + markPad * 2, height: title.height + markPad * 2 };
  $('watermark').setAttribute('viewBox', `${markBounds.x} ${markBounds.y} ${markBounds.width} ${markBounds.height}`);
  const defs = svg('defs', {});
  const mask = svg('mask', { id: 'mark-clearance', maskUnits: 'userSpaceOnUse', ...markBounds, 'mask-type': 'luminance' });
  mask.append(svg('rect', { ...markBounds, fill: 'white' }));
  const rings = svg('g', { id: 'mark-rings', fill: 'none', stroke: settings.markOutline, 'stroke-width': 48, 'stroke-linejoin': 'round', mask: 'url(#mark-clearance)' });
  const letters = svg('g', { fill: '#f2eee9' });
  for (const glyph of title.glyphs) {
    mask.append(svg('path', { d: glyph.path, fill: 'black', stroke: 'black', 'stroke-width': 24, 'stroke-linejoin': 'round' }));
    if (glyph.initial) rings.append(svg('path', { d: glyph.path }));
    letters.append(svg('path', { d: glyph.path }));
  }
  defs.append(mask); $('watermark').append(defs, rings, letters);

  // Same hash, smooth value noise, warp and folded ramp as the title study.
  function hash(x, y) {
    let qx = fract(x * .1031), qy = fract(y * .103);
    const d = qx * (qy + 33.33) + qy * (qx + 33.33);
    qx += d; qy += d; return fract((qx + qy) * qx);
  }
  function noise(x, y) {
    const ix = Math.floor(x), iy = Math.floor(y), fx = fract(x), fy = fract(y);
    const u = fx * fx * (3 - 2 * fx), v = fy * fy * (3 - 2 * fy);
    return mix(mix(hash(ix, iy), hash(ix + 1, iy), u), mix(hash(ix, iy + 1), hash(ix + 1, iy + 1), u), v);
  }
  function ramp(at, c) {
    const x = fract(at) * (palette.length - 1), i = Math.floor(x);
    return mix(palette[i][c], palette[i + 1][c], fract(x));
  }
  function paintTexture() {
    const t = settings.reduced ? 4 : evolution;
    const p = settings.reduced ? .626 : phase;
    for (let y = 0; y < texture.height; y++) for (let x = 0; x < texture.width; x++) {
      const px = x * settings.scale, py = y * settings.scale;
      const dx = noise(px * .6 + t, py * .6 + t * .23) - .5;
      const dy = noise(px * .6 + 8.7 - t * .31, py * .6 + 2.3 + t) - .5;
      const wx = px + dx * 1.35, wy = py + dy * 1.35;
      const folds = noise(wx - t * .15, wy + t * .37);
      const q = fract((wx * .18 + wy * .12 + 1.25 * folds) * 2 - 97 / 360);
      const seam = clamp((q - .5) / .5), u = seam * seam * (3 - 2 * seam);
      // One field, sampled once: the green, coral and blue ramps share
      // exactly the same folds, coordinates and clock across their boundaries.
      const green = clamp(.25 + .5 * x / texture.width + Math.cos(q * Math.PI * 2) * settings.contrast * .5);
      const at = (y * texture.width + x) * 4;
      for (let c = 0; c < 3; c++) {
        const vivid = mix(ramp(p + .43 * q, c), ramp(p + .43 * (q - 1), c), u);
        pixels.data[at + c] = mix(mix(normal.jade[c], normal.mint[c], green), vivid, fullMix);
        for (const accent of accents) accent.pixels.data[at + c] = mix(accent.from[c], accent.to[c], green);
      }
      pixels.data[at + 3] = 255;
      for (const accent of accents) accent.pixels.data[at + 3] = 255;
    }
    textureContext.putImageData(pixels, 0, 0);
    for (const accent of accents) accent.context.putImageData(accent.pixels, 0, 0);
  }
  function surface(canvas, w, h) {
    const dpr = Math.min(devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(w * dpr)), height = Math.max(1, Math.round(h * dpr));
    if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
    const ctx = canvas.getContext('2d'); ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.clearRect(0, 0, w, h);
    return ctx;
  }
  function paintBars() {
    for (const bar of bars) {
      const canvas = bar.querySelector('canvas'), width = bar.clientWidth, height = settings.height;
      canvas.style.height = `${height}px`;
      const ctx = surface(canvas, width, height);
      ctx.save(); ctx.beginPath(); ctx.roundRect(0, 0, width, height, Math.min(settings.radius, height / 2)); ctx.clip();
      ctx.fillStyle = settings.track; ctx.fillRect(0, 0, width, height);
      const segment = (image, from, to, alpha = 1) => {
        if (to <= from || alpha <= 0) return;
        ctx.save(); ctx.beginPath(); ctx.rect(width * from / 100, 0, width * (to - from) / 100, height); ctx.clip();
        ctx.globalAlpha = alpha;
        // Clip a full-bar field; stretching it into the segment would make
        // the pattern slide and change scale as life catches up.
        ctx.drawImage(image, 0, 0, width, height); ctx.restore();
      };
      segment(accents[0].canvas, Math.max(fill, life), trail, trailAlpha);
      segment(accents[1].canvas, fill, life);
      ctx.save(); ctx.beginPath(); ctx.rect(0, 0, width * fill / 100, height); ctx.clip();
      ctx.drawImage(texture, 0, 0, width, height);
      if (life <= settings.low) {
        ctx.globalAlpha = .9; ctx.fillStyle = settings.warning; ctx.fillRect(0, 0, width, height); ctx.globalAlpha = 1;
      }
      ctx.restore(); ctx.restore();
    }
  }
  function updateLifeLabel() {
    $('life').value = life; $('life-value').textContent = `${Math.round(life)}%`;
    const state = life === 0 ? 'Empty' : life === 100 ? 'Full' : life <= settings.low ? 'Low' : 'Normal';
    $('life-status').textContent = `${Math.round(life)}% · ${state}`;
    for (const bar of bars) { bar.setAttribute('aria-valuenow', life); bar.setAttribute('aria-valuetext', `${Math.round(life)}% — ${state}`); }
  }
  function sampleLife() {
    if (recovery) {
      const t = settings.gain ? (clock - recovery.at) / settings.gain : 1;
      fill = mix(recovery.from, life, ease(t)); if (t >= 1) { fill = life; recovery = null; }
    }
    if (damage) {
      if (settings.reduced) {
        trail = damage.from; trailAlpha = 1 - clamp((clock - damage.at) / 200);
        if (trailAlpha === 0) damage = null;
      } else {
        const t = (clock - damage.at - settings.delay) / settings.loss;
        trail = mix(damage.from, damage.to, ease(t)); trailAlpha = 1;
        if (t >= 1) damage = null;
      }
    }
    if (!damage) { trail = fill; trailAlpha = 0; }
    trail = Math.max(fill, trail);
  }
  function setLife(value) {
    sampleLife(); const old = life; life = clamp(value, 0, 100);
    if (life < old) {
      damage = { from: Math.max(old, trail, fill), to: life, at: clock };
      fill = life; recovery = null; fullMix = 0; trailAlpha = 1; trail = damage.from;
    } else if (life > old) {
      recovery = settings.reduced || settings.gain === 0 ? null : { from: fill, at: clock };
      if (!recovery) fill = life;
    }
    updateLifeLabel(); dirty = true;
  }
  function setCombo(value) {
    const oldTier = Math.floor(settings.combo / 100);
    settings.combo = Math.round(clamp(value, 0, 99999));
    if (Math.floor(settings.combo / 100) > oldTier) comboPulse = clock;
    sync(); save();
  }
  function paintCombo() {
    const w = comboCanvas.clientWidth, h = 116;
    const ctx = surface(comboCanvas, w, h);
    if (settings.combo < 4) return;
    const tier = Math.min(10, Math.floor(settings.combo / 100));
    const pulse = settings.reduced ? 1 : 1 + .07 * (1 - clamp((clock - comboPulse) / 280));
    const family = settings.font === 'dosis' ? 'Dosis' : 'IosevkaHUD';
    const label = String(settings.combo);
    const inverted = tier === 10 && settings.thousandStyle === 'inverted';
    const gap = settings.comboGap;
    const outline = (3.5 + Math.min(8, tier - 1) * .08) * settings.tierStrength * (tier === 10 ? settings.thousandOutline : 1);
    const wantedSize = settings.comboSize * (tier === 10 ? settings.thousandScale : 1);
    ctx.font = `${settings.numberWeight} ${wantedSize}px ${family}`;
    const metrics = ctx.measureText(label);
    // Fit the entire outlined, briefly popped numeral at narrow widths and
    // extreme control settings, reserving room for both gap and outer ring.
    const margin = 2 * (gap + outline + 2);
    const fit = Math.min(1, (w / pulse - margin) / Math.max(1, metrics.width),
      (h / pulse - margin) / Math.max(1, metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent));
    const size = wantedSize * Math.max(.1, fit);
    ctx.font = `${settings.numberWeight} ${size}px ${family}`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.translate(w / 2, h / 2); ctx.scale(pulse, pulse);
    const gradient = ctx.createLinearGradient(-w * .25, -h / 2, w * .25, h / 2);
    const base = tier === 10 ? (settings.reduced ? .626 : phase) : (tier - 1) / 10;
    for (let i = 0; i <= 4; i++) gradient.addColorStop(i / 4, `rgb(${[0, 1, 2].map(c => Math.round(ramp(base + i * .09, c))).join(',')})`);
    if (tier > 0 && settings.tierStrength > 0) {
      // A separate transparent layer gives EVERY outlined tier a real gap,
      // including over bright backgrounds; no dark stroke painted in the gap.
      const lc = surface(comboLayer, w, h);
      lc.font = ctx.font; lc.textAlign = 'center'; lc.textBaseline = 'middle'; lc.lineJoin = 'round';
      const ringGradient = lc.createLinearGradient(w * .25, 0, w * .75, h);
      for (let i = 0; i <= 4; i++) ringGradient.addColorStop(i / 4, `rgb(${[0, 1, 2].map(c => Math.round(ramp(base + i * .09, c))).join(',')})`);
      lc.strokeStyle = inverted ? '#f2eee9' : ringGradient;
      lc.lineWidth = 2 * (gap + outline);
      lc.strokeText(label, w / 2, h / 2);
      lc.globalCompositeOperation = 'destination-out'; lc.lineWidth = 2 * gap;
      if (gap > 0) lc.strokeText(label, w / 2, h / 2);
      lc.fillText(label, w / 2, h / 2);
      lc.globalCompositeOperation = 'source-over';
      ctx.drawImage(comboLayer, -w / 2, -h / 2, w, h);
    }
    ctx.fillStyle = inverted ? gradient : '#f2eee9'; ctx.fillText(label, 0, 0);
  }
  function paintField() {
    const w = fieldCanvas.clientWidth, h = fieldCanvas.clientHeight, ctx = surface(fieldCanvas, w, h);
    const lane = Math.min(62, w / 6), left = w / 2 - lane * 2;
    ctx.fillStyle = '#05020d55'; ctx.fillRect(left - 12, 72, lane * 4 + 24, h - 72);
    ctx.strokeStyle = '#c9c2bb0b'; ctx.lineWidth = 1;
    for (let i = 0; i <= 4; i++) { ctx.beginPath(); ctx.moveTo(left + lane * i, 72); ctx.lineTo(left + lane * i, h); ctx.stroke(); }
    function arrow(x, y, angle, active) {
      ctx.save(); ctx.translate(x, y); ctx.rotate(angle); ctx.scale(lane / 62, lane / 62);
      ctx.beginPath(); ctx.moveTo(0, -18); ctx.lineTo(18, 0); ctx.lineTo(9, 0); ctx.lineTo(9, 15); ctx.lineTo(-9, 15); ctx.lineTo(-9, 0); ctx.lineTo(-18, 0); ctx.closePath();
      ctx.strokeStyle = active ? '#83b8c47a' : '#c9c2bb66'; ctx.lineWidth = 2; ctx.stroke();
      if (active) { ctx.fillStyle = '#83b8c419'; ctx.fill(); } ctx.restore();
    }
    const angles = [-Math.PI / 2, Math.PI, 0, Math.PI / 2];
    angles.forEach((angle, i) => arrow(left + lane * (i + .5), 116, angle, false));
    [0, 3, 1, 2, 0, 2].forEach((col, i) => arrow(left + lane * (col + .5), 174 + i * 64, angles[col], true));
  }
  const demo = [[0, 78], [900, 60], [1080, 48], [2200, 56], [2550, 64], [2900, 72], [3250, 80], [3600, 88], [3950, 96], [4300, 100], [7200, 82], [8200, 18], [9800, 0], [11100, 38]];
  function stopDemo() { demoStart = null; $('demo').textContent = 'Run demo'; $('demo').setAttribute('aria-pressed', 'false'); }
  $('demo').addEventListener('click', () => {
    if (demoStart !== null) stopDemo();
    else { demoStart = clock; demoIndex = 0; $('demo').textContent = 'Stop demo'; $('demo').setAttribute('aria-pressed', 'true'); }
  });
  for (const [id, value] of [['damage', () => life - 18], ['recover', () => life + 8], ['full', () => 100], ['danger', () => 18]]) {
    $(id).addEventListener('click', () => { stopDemo(); setLife(value()); });
  }
  $('life').addEventListener('input', () => { stopDemo(); setLife(Number($('life').value)); });
  $('combo-value').addEventListener('input', () => { if ($('combo-value').value !== '') setCombo(Number($('combo-value').value)); });
  $('next-tier').addEventListener('click', () => setCombo((Math.floor(settings.combo / 100) + 1) * 100));
  $('thousand').addEventListener('click', () => setCombo(1000));
  $('reset').addEventListener('click', () => {
    settings = { ...defaults, reduced: systemMotion.matches }; stopDemo();
    life = fill = trail = 78; damage = recovery = null; trailAlpha = fullMix = 0; evolution = 4; phase = .626;
    sync(); updateLifeLabel(); save(); $('status').textContent = 'Study defaults restored.';
  });
  $('export').addEventListener('click', () => {
    const data = JSON.stringify({ study: 'ddi-play-hud', version: 1, settings }, null, 2) + '\n';
    const url = URL.createObjectURL(new Blob([data], { type: 'application/json' }));
    const a = document.createElement('a'); a.href = url; a.download = 'dito-play-hud.json'; a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    $('status').textContent = 'Exported the current design settings.';
  });
  $('import').addEventListener('change', async event => {
    const file = event.target.files[0]; if (!file) return;
    try {
      if (file.size > 32768) throw new Error('This settings file is too large.');
      const data = JSON.parse(await file.text());
      if (data.study !== 'ddi-play-hud' || data.version !== 1) throw new Error('Choose a version 1 Play HUD settings export.');
      settings = validate(data.settings); stopDemo(); damage = recovery = null; fill = trail = life; trailAlpha = 0;
      sync(); updateLifeLabel(); save(); $('status').textContent = 'Imported design settings.';
    } catch (error) { $('status').textContent = `Could not import: ${error.message}`; }
    event.target.value = '';
  });
  systemMotion.addEventListener('change', () => {
    settings.reduced = systemMotion.matches; damage = recovery = null; fill = trail = life; trailAlpha = 0; sync(); save();
  });
  function tick(now) {
    const dt = document.hidden ? 0 : Math.min(now - previous, 80); previous = now; clock += dt;
    if (demoStart !== null) {
      const elapsed = clock - demoStart;
      while (demoIndex < demo.length && elapsed >= demo[demoIndex][0]) setLife(demo[demoIndex++][1]);
      if (elapsed > 12800) { demoStart = clock; demoIndex = 0; }
    }
    const oldVisual = [fill, trail, trailAlpha];
    sampleLife();
    const fullTarget = life === 100 && fill >= 99.999 ? 1 : 0;
    const nextMix = settings.reduced || settings.fullBlend === 0 ? fullTarget : fullMix + clamp(fullTarget - fullMix, -dt / settings.fullBlend, dt / settings.fullBlend);
    const transitioning = !!damage || !!recovery || nextMix !== fullMix || clock - comboPulse < 300 ||
      oldVisual.some((value, i) => value !== [fill, trail, trailAlpha][i]);
    // Preserve the last transition frame even when it falls between paints.
    dirty ||= transitioning;
    fullMix = nextMix;
    const moving = !settings.still && !settings.reduced;
    if (moving) { evolution += dt / 1000 * mix(settings.speed, settings.fullSpeed, fullMix); phase = fract(phase - dt / 1000 / settings.paletteSeconds); }
    if (!document.hidden && (dirty || transitioning || moving) && now - lastPaint >= 1000 / 30) {
      paintTexture(); paintBars(); paintCombo();
      if (dirty) { paintField(); updateLifeLabel(); }
      dirty = false; lastPaint = now;
    }
    requestAnimationFrame(tick);
  }
  new ResizeObserver(() => { dirty = true; }).observe($('stage'));
  new ResizeObserver(() => { dirty = true; }).observe(document.querySelector('.detail-space'));
  document.fonts.ready.then(() => { dirty = true; });
  document.fonts.addEventListener('loadingdone', () => { dirty = true; });
  sync(); updateLifeLabel(); requestAnimationFrame(tick);
  // Read-only diagnostics for browser verification; editing goes through controls.
  window.hudStudy = { snapshot: () => ({ settings: { ...settings }, life, fill, trail, trailAlpha, fullMix, evolution, phase, demo: demoStart !== null }) };
})();
