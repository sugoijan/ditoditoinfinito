// Title study only. Infinito's Vivid material translated to JavaScript;
// see README.md for the deliberate title-specific coordinate and phase choices.
(() => {
  const root = document.getElementById('ddi-title-study');
  const data = window.DDI_TITLE;
  const large = root.querySelector('.title-large');
  const small = root.querySelector('.title-small');
  const menu = root.querySelector('.title-menu');
  const motion = root.querySelector('#title-motion');
  const outline = root.querySelector('#title-outline');
  const gap = root.querySelector('#title-gap');
  const fill = root.querySelector('#title-fill');
  const phase = root.querySelector('#title-phase');
  const status = root.querySelector('#title-status');
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const maxOutline = 24, maxGap = 12;
  // Reserve the maximum extent so changing either control never crops or
  // rescales the wordmark. Field coordinates remain anchored to each glyph.
  const padding = maxOutline + maxGap + 2;
  const titleView = { x: -padding, y: -padding,
    width: data.title.width + padding * 2, height: data.title.height + padding * 2 };
  const state = { outline: 12, gap: 12, fill: 'filled', phase: 1, seconds: 4 };
  const palette = data.palette.map(hex => [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16)));
  const paths = data.title.glyphs.map(glyph => new Path2D(glyph.path));
  const uiPaths = data.ui.glyphs.map(glyph => new Path2D(glyph.path));
  let playing = false, frame = 0, previous = 0, fields = [];
  const fract = x => x - Math.floor(x);
  const mix = (a, b, t) => a + (b - a) * t;

  // Same hash and smooth value noise as material_prelude.wgsl.
  function hash(x, y) {
    let qx = fract(x * 0.1031), qy = fract(y * 0.1030);
    const d = qx * (qy + 33.33) + qy * (qx + 33.33);
    qx += d; qy += d;
    return fract((qx + qy) * qx);
  }
  function noise(x, y) {
    const ix = Math.floor(x), iy = Math.floor(y);
    const fx = fract(x), fy = fract(y);
    const u = fx * fx * (3 - 2 * fx), v = fy * fy * (3 - 2 * fy);
    return mix(mix(hash(ix, iy), hash(ix + 1, iy), u),
      mix(hash(ix, iy + 1), hash(ix + 1, iy + 1), u), v);
  }
  function ramp(t, channel) {
    const x = fract(t) * (palette.length - 1), i = Math.floor(x);
    return mix(palette[i][channel], palette[i + 1][channel], fract(x));
  }
  function paintField(glyph, index) {
    const [left, top, right, bottom] = glyph.bounds;
    const pad = padding;
    const width = Math.ceil(right - left + pad * 2), height = Math.ceil(bottom - top + pad * 2);
    const canvas = document.createElement('canvas');
    canvas.width = width; canvas.height = height;
    const ctx = canvas.getContext('2d');
    const image = ctx.createImageData(width, height);
    const t = state.seconds * 0.3;
    // A title has no note timestamps. Give the initials quarter-cycle offsets.
    const localPhase = state.phase - index * 0.25;
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        // A 96-unit letter cap occupies a 64-unit noteskin field.
        const px = ((x + 0.5 - pad - (right - left) / 2) * 64 / 96) / 1.089 * 0.021;
        const py = ((y + 0.5 - pad - (bottom - top) / 2) * 64 / 96 - 8.712) / 1.089 * 0.021;
        const dx = noise(px * 0.6 + t, py * 0.6 + t * 0.23) - 0.5;
        const dy = noise(px * 0.6 + 8.7 - t * 0.31, py * 0.6 + 2.3 + t) - 0.5;
        const wx = px + dx * 1.35, wy = py + dy * 1.35;
        const folds = noise(wx - t * 0.15, wy + t * 0.37);
        const q = fract((wx * 0.18 + wy * 0.12 + 1.25 * folds) * 2 - 97 / 360);
        const s = Math.max(0, Math.min(1, (q - 0.5) / 0.5));
        const u = s * s * (3 - 2 * s);
        const at = (y * width + x) * 4;
        for (let c = 0; c < 3; c++) {
          image.data[at + c] = mix(ramp(localPhase + 0.43 * q, c),
            ramp(localPhase + 0.43 * (q - 1), c), u);
        }
        image.data[at + 3] = 255;
      }
    }
    ctx.putImageData(image, 0, 0);
    return { canvas, x: left - pad, y: top - pad, width, height };
  }
  function surface(canvas, geometry) {
    const width = canvas.getBoundingClientRect().width;
    const height = width * geometry.height / geometry.width;
    const dpr = Math.min(devicePixelRatio || 1, 3);
    canvas.style.height = `${height}px`;
    canvas.width = Math.max(1, Math.round(width * dpr));
    canvas.height = Math.max(1, Math.round(height * dpr));
    const ctx = canvas.getContext('2d');
    ctx.scale(canvas.width / geometry.width, canvas.height / geometry.height);
    ctx.translate(-(geometry.x || 0), -(geometry.y || 0));
    return ctx;
  }
  function drawTitle(canvas) {
    const ctx = surface(canvas, titleView);
    const layer = document.createElement('canvas');
    layer.width = canvas.width; layer.height = canvas.height;
    const lc = layer.getContext('2d');
    lc.setTransform(ctx.getTransform());
    let initial = 0;
    // First paint all three expanded silhouettes behind the entire title.
    data.title.glyphs.forEach((glyph, i) => {
      const path = paths[i];
      if (glyph.initial) {
        const field = fields[initial++];
        const mask = document.createElement('canvas');
        mask.width = field.width * 2; mask.height = field.height * 2;
        const mc = mask.getContext('2d');
        mc.scale(2, 2); mc.translate(-field.x, -field.y);
        mc.lineJoin = 'round'; mc.lineWidth = (state.gap + state.outline) * 2;
        mc.strokeStyle = '#fff'; mc.stroke(path);
        mc.globalCompositeOperation = 'source-in';
        mc.drawImage(field.canvas, field.x, field.y);
        lc.drawImage(mask, field.x, field.y, field.width, field.height);
      }
    });
    // Then subtract a clearance around EVERY glyph, including adjacent i/n
    // and their counters/dots. This is real transparency, not background paint.
    lc.globalCompositeOperation = 'destination-out';
    lc.fillStyle = '#000'; lc.strokeStyle = '#000'; lc.lineJoin = 'round';
    lc.lineWidth = state.gap * 2;
    paths.forEach(path => {
      lc.fill(path);
      if (state.gap > 0) lc.stroke(path);
    });
    ctx.drawImage(layer, titleView.x, titleView.y, titleView.width, titleView.height);
    // Foreground letters are always the last pass, independent of word order.
    data.title.glyphs.forEach((glyph, i) => {
      ctx.fillStyle = glyph.initial && state.fill === 'hollow' ? '#14121a' : '#f2eee9';
      ctx.fill(paths[i]);
    });
  }
  function render(repaint = true) {
    if (repaint) fields = data.title.glyphs.filter(g => g.initial).map(paintField);
    drawTitle(large); drawTitle(small);
    const ctx = surface(menu, data.ui);
    ctx.fillStyle = '#c9c2bb'; uiPaths.forEach(path => ctx.fill(path));
    outline.value = state.outline; gap.value = state.gap; fill.value = state.fill; phase.value = state.phase;
    root.querySelector('#title-outline-value').textContent = state.outline;
    root.querySelector('#title-gap-value').textContent = state.gap;
    motion.textContent = playing ? 'Pause motion' : 'Animate title';
    motion.setAttribute('aria-pressed', String(playing));
  }
  function saveState() {
    window.openai?.setWidgetState?.({ modelContent: { ...state }, privateContent: { ...state } }).catch(() => {});
  }
  function tick(now) {
    if (!playing) return;
    if (now - previous >= 1000 / 24) {
      const dt = document.hidden ? 0 : Math.min((now - previous) / 1000, 0.1);
      previous = now;
      state.seconds += dt;
      state.phase = fract(state.phase - dt / 4);
      if (!document.hidden) render();
    }
    frame = requestAnimationFrame(tick);
  }
  function stop() { playing = false; cancelAnimationFrame(frame); render(false); }
  motion.addEventListener('click', () => {
    if (playing) stop();
    else { playing = true; previous = performance.now(); render(false); frame = requestAnimationFrame(tick); }
    saveState();
  });
  reduced.addEventListener('change', () => { if (reduced.matches) stop(); });
  outline.addEventListener('input', () => { state.outline = Number(outline.value); render(false); });
  outline.addEventListener('change', saveState);
  gap.addEventListener('input', () => { state.gap = Number(gap.value); render(false); });
  gap.addEventListener('change', saveState);
  fill.addEventListener('change', () => { state.fill = fill.value; render(false); saveState(); });
  phase.addEventListener('input', () => { const value = Number(phase.value); stop(); state.phase = value; render(); });
  phase.addEventListener('change', saveState);

  function exportSvg() {
    const { x, y, width, height } = titleView;
    const parts = [`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="${x} ${y} ${width} ${height}">`,
      '<title>Dito Dito Infinito — Dosis with Infinito Vivid initials</title>',
      `<desc>Dosis 800. Outline ${state.outline}; gap ${state.gap}; phase ${state.phase}; field seconds ${state.seconds}; initials ${state.fill}. Vector letter shapes with embedded procedural color fields. Transparent clearance protects every letter.</desc>`,
      `<defs><mask id="letter-clearance" maskUnits="userSpaceOnUse" x="${x}" y="${y}" width="${width}" height="${height}" style="mask-type:luminance"><rect x="${x}" y="${y}" width="${width}" height="${height}" fill="white"/>`];
    for (const glyph of data.title.glyphs) {
      parts.push(`<path d="${glyph.path}" fill="black" stroke="black" stroke-width="${state.gap * 2}" stroke-linejoin="round"/>`);
    }
    parts.push('</mask></defs><g mask="url(#letter-clearance)">');
    let initial = 0;
    for (const glyph of data.title.glyphs) {
      if (glyph.initial) {
        const index = initial++, field = fields[index];
        parts.push(`<defs><mask id="ring${index}" maskUnits="userSpaceOnUse" x="${field.x}" y="${field.y}" width="${field.width}" height="${field.height}"><path d="${glyph.path}" fill="none" stroke="white" stroke-width="${(state.gap + state.outline) * 2}" stroke-linejoin="round"/></mask></defs>`,
          `<image x="${field.x}" y="${field.y}" width="${field.width}" height="${field.height}" href="${field.canvas.toDataURL('image/png')}" mask="url(#ring${index})"/>`);
      }
    }
    parts.push('</g>');
    for (const glyph of data.title.glyphs) {
      parts.push(`<path d="${glyph.path}" fill="${glyph.initial && state.fill === 'hollow' ? '#14121a' : '#f2eee9'}"/>`);
    }
    return parts.join('\n') + '\n</svg>\n';
  }
  root.querySelector('#title-export').addEventListener('click', () => {
    const url = URL.createObjectURL(new Blob([exportSvg()], { type: 'image/svg+xml' }));
    const link = document.createElement('a'); link.href = url; link.download = 'dito-dito-infinito-dosis.svg'; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    status.textContent = 'Saved the current colors and outline as an SVG.';
  });
  function restore(saved) {
    const s = saved?.privateContent;
    if (s) {
      if (Number.isFinite(s.outline)) state.outline = Math.min(maxOutline, Math.max(1, s.outline));
      if (Number.isFinite(s.gap)) state.gap = Math.min(maxGap, Math.max(0, s.gap));
      if (['filled', 'hollow'].includes(s.fill)) state.fill = s.fill;
      if (Number.isFinite(s.phase)) state.phase = Math.min(1, Math.max(0, s.phase));
      if (Number.isFinite(s.seconds)) state.seconds = Math.max(0, s.seconds);
    }
    render();
  }
  restore(window.openai?.widgetState);
  if (!reduced.matches) {
    playing = true; previous = performance.now(); render(false);
    frame = requestAnimationFrame(tick);
  }
  window.addEventListener('openai:set_globals', event => { if (event.detail?.globals?.widgetState) restore(event.detail.globals.widgetState); });
  new ResizeObserver(() => render(false)).observe(root.querySelector('.title-stage'));
  // Read-only inspection/export for the local verification script.
  root.titleStudy = { exportSvg, snapshot: () => ({ ...state, playing }) };
})();
