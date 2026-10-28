// The loader's art, on one canvas over the page while the game arrives
// (index.html's boot script lays it out, starts it, feeds it the
// progress and stops it): the title as the start menu draws it, the
// menu's outline rain behind it and the progress bar painted the same
// way, all on the game's clock so that the menu fades in under them
// unchanged (docs/plans/start-menu.md; AGENTS.md, "Deployment").
//
// The paint is Infinito's Vivid material ported from its WGSL
// (assets/noteskins/dance/Infinito/_spiral material.wgsl with
// crates/render/src/vivid.wgsl's parameters, the noise of
// crates/render/src/material_prelude.wgsl): CPU doubles for the GPU's
// floats, the same math otherwise. The title's lettering, its rings and
// its clearance are the atlas's (tools/title-design/generate.py), the
// rain is crates/theme/src/rain.rs particle for particle, both from
// the data boot_title.py writes beside this file (boot-art-data.js).
(() => {
  const D = window.__DDI_ART_DATA;
  const palette = D.palette.map((h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)));
  const fract = (x) => x - Math.floor(x);
  const mix = (a, b, t) => a + (b - a) * t;
  const clamp01 = (x) => Math.min(1, Math.max(0, x));
  // material_prelude.wgsl: hash12 and the smooth value noise.
  const hash = (x, y) => {
    let qx = fract(x * 0.1031), qy = fract(y * 0.103);
    const d = qx * (qy + 33.33) + qy * (qx + 33.33);
    qx += d;
    qy += d;
    return fract((qx + qy) * qx);
  };
  const noise = (x, y) => {
    const ix = Math.floor(x), iy = Math.floor(y), fx = x - ix, fy = y - iy;
    const u = fx * fx * (3 - 2 * fx), v = fy * fy * (3 - 2 * fy);
    return mix(mix(hash(ix, iy), hash(ix + 1, iy), u), mix(hash(ix, iy + 1), hash(ix + 1, iy + 1), u), v);
  };
  // vivid.wgsl: the eight-stop closed palette.
  const ramp = (t, c) => {
    const x = fract(t) * 7, i = Math.min(Math.floor(x), 6);
    return mix(palette[i][c], palette[i + 1][c], x - i);
  };
  // The material's paint at field coordinates (fx, fy): its seconds and
  // palette phase as ddi_actor::Vivid::clock gives them, the color's
  // bytes into out[at..at + 3].
  const paint = (fx, fy, seconds, phase, out, at) => {
    const px = fx / 1.089 * 0.021, py = (fy - 8.712) / 1.089 * 0.021, t = seconds * 0.3;
    const dx = noise(px * 0.6 + t, py * 0.6 + t * 0.23) - 0.5;
    const dy = noise(px * 0.6 + 8.7 - t * 0.31, py * 0.6 + 2.3 + t) - 0.5;
    const wx = px + dx * 1.35, wy = py + dy * 1.35;
    const folds = noise(wx - t * 0.15, wy + t * 0.37);
    const q = fract((wx * 0.18 + wy * 0.12 + 1.25 * folds) * 2 - 97 / 360);
    const s = clamp01((q - 0.5) / 0.5), u = s * s * (3 - 2 * s);
    for (let c = 0; c < 3; c++) {
      out[at + c] = mix(ramp(phase + 0.43 * q, c), ramp(phase + 0.43 * (q - 1), c), u);
    }
  };
  // The clock the game's live art runs on (ddi_ui::screens::Live, the
  // start menu's rain): the wall clock, seconds since the epoch, so a
  // reload does not start the art over from the same state. The title
  // takes it wrapped at a multiple of the palette's four
  // (ddi_actor::Vivid::clock: the material's seconds 4 past it, its phase
  // a palette backward per four); the focus outline's paint, which the
  // bar borrows (crates/theme/src/active.rs: Active.EvolutionSpeed 8,
  // the field a square as wide as the viewport), runs its seconds eight
  // times as fast.
  const now = () => (performance.timeOrigin + performance.now()) / 1000;
  const titleClock = (time) => {
    const t = time % 4096;
    return [4 + t, fract(1 - t / 4)];
  };
  const activeClock = (time) => {
    const t = time % 4096;
    return [4 + 8 * t, fract(1 - t / 4)];
  };

  // The lettering: each distinct outline once, placed per glyph.
  const shapes = {};
  for (const [k, d] of D.glyphs) shapes[k] = new Path2D(d);
  const placed = D.uses.map(([k, x, y]) => {
    const p = new Path2D();
    p.addPath(shapes[k], new DOMMatrix().translate(x, y));
    return p;
  });
  const initials = D.initials.map((n) => ({ path: placed[n], bounds: D.bounds[n] }));
  const PAD = D.pad, GAP = 12, OUTLINE = 12;
  const arrow = new Path2D(D.arrow), onigiri = new Path2D(D.onigiri);

  // The theme's colors, the page's variables (target/seo/theme.css).
  const color = (name, fallback) =>
    getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
  let canvas = null, ctx = null, layout = null, raf = 0, reduced = false;
  // The bar: what is reported (`target`) and what is drawn (`shown`),
  // which follows CAP of it at a bounded pace (the reports come in
  // steps whose size and timing depend on the link and the device; the
  // fill moves evenly regardless, between FLOOR and MAXV a second,
  // closing a gap by RATE of it a second), creeps on by up to AHEAD
  // while a report is late and fills the rest over FINISH_MS once the
  // game is ready (`finish`). Reduced motion takes each step whole.
  const CAP = 0.95, FINISH_MS = 240, RATE = 4, FLOOR = 0.1, MAXV = 1, CREEP = 0.3, AHEAD = 0.04;
  let target = 0.04, shown = 0.04, finishing = 0, lastStep = 0;
  const step = (nowMs) => {
    const dt = lastStep ? Math.min(0.1, (nowMs - lastStep) / 1000) : 0;
    lastStep = nowMs;
    if (finishing) {
      shown = reduced ? 1 : Math.min(1, shown + dt * 1000 / FINISH_MS * finishing);
      return;
    }
    const goal = CAP * target;
    if (reduced) {
      shown = Math.max(shown, goal);
      return;
    }
    const gap = goal - shown;
    if (gap > 0) {
      shown += Math.min(gap, dt * Math.min(MAXV, Math.max(RATE * gap, FLOOR)));
    } else {
      shown += dt * CREEP * Math.max(0, Math.min(CAP, goal + AHEAD) - shown);
    }
  };
  const layer = document.createElement("canvas"), lc = layer.getContext("2d");
  const rings = initials.map(() => {
    const c = document.createElement("canvas");
    return { c, cx: c.getContext("2d"), field: document.createElement("canvas"), image: null, box: null };
  });
  const bar = { c: document.createElement("canvas"), image: null };

  const layoutTitle = () => {
    const { title, dpr } = layout;
    const sc = title.w / D.title.w;
    layer.width = Math.ceil(title.w * dpr);
    layer.height = Math.ceil(title.h * dpr);
    // Each ring's field: the initial's bounds, the ring's reach around
    // them, at most half the device's pixels and never finer than a
    // title unit (the field is smooth noise).
    const fs = Math.max(1, 2 / (sc * dpr));
    for (const [n, r] of rings.entries()) {
      const [l, t, rt, b] = initials[n].bounds, reach = GAP + OUTLINE + 2;
      const box = { x: l - reach, y: t - reach, w: rt - l + 2 * reach, h: b - t + 2 * reach };
      box.cols = Math.ceil(box.w / fs);
      box.rows = Math.ceil(box.h / fs);
      box.fs = fs;
      r.box = box;
      r.field.width = box.cols;
      r.field.height = box.rows;
      r.image = r.field.getContext("2d").createImageData(box.cols, box.rows);
      r.c.width = Math.ceil(box.w * sc * dpr);
      r.c.height = Math.ceil(box.h * sc * dpr);
    }
  };

  const drawTitle = (time) => {
    const { title, dpr } = layout;
    const sc = title.w / D.title.w, k = sc * dpr;
    const [seconds, phase] = titleClock(time);
    lc.setTransform(k, 0, 0, k, PAD * k, PAD * k);
    lc.clearRect(-PAD, -PAD, D.title.w, D.title.h);
    for (const [n, r] of rings.entries()) {
      const { box, image } = r, [l, t, rt, b] = initials[n].bounds;
      const cx = (l + rt) / 2, cy = (t + b) / 2, data = image.data;
      // The field over the ring's box: title units to the field's
      // (a 96-unit capital in a 64-unit field, centered on the initial),
      // this initial a quarter palette behind the one before.
      for (let j = 0, at = 0; j < box.rows; j++) {
        const fy = (box.y + (j + 0.5) * box.fs - cy) * 2 / 3;
        for (let i = 0; i < box.cols; i++, at += 4) {
          paint((box.x + (i + 0.5) * box.fs - cx) * 2 / 3, fy, seconds, fract(phase - n / 4), data, at);
          data[at + 3] = 255;
        }
      }
      r.field.getContext("2d").putImageData(image, 0, 0);
      // The ring: the initial stroked to the outline's reach, the field through it.
      const rc = r.cx;
      rc.setTransform(k, 0, 0, k, -box.x * k, -box.y * k);
      rc.globalCompositeOperation = "source-over";
      rc.clearRect(box.x, box.y, box.w, box.h);
      rc.lineJoin = "round";
      rc.lineWidth = (GAP + OUTLINE) * 2;
      rc.strokeStyle = "#fff";
      rc.stroke(initials[n].path);
      rc.globalCompositeOperation = "source-in";
      rc.drawImage(r.field, box.x, box.y, box.w, box.h);
      lc.drawImage(r.c, box.x, box.y, box.w, box.h);
    }
    // The clearance around every letter, then the lettering.
    lc.globalCompositeOperation = "destination-out";
    lc.fillStyle = "#000";
    lc.strokeStyle = "#000";
    lc.lineJoin = "round";
    lc.lineWidth = GAP * 2;
    for (const p of placed) {
      lc.fill(p);
      lc.stroke(p);
    }
    lc.globalCompositeOperation = "source-over";
    lc.fillStyle = color("--text", "#f2eee9");
    for (const p of placed) lc.fill(p);
    ctx.drawImage(layer, title.x, title.y, title.w, title.h);
  };

  // crates/theme/src/rain.rs, particle for particle: the study's seed,
  // a shape and direction kept until the particle wraps, the quiet
  // around the middle, in the start menu's layout units scaled to CSS.
  const seed = (i, salt) => fract(Math.sin(i * 127.1 + salt * 311.7) * 43758.5453);
  const mod = (a, n) => a - Math.floor(a / n) * n;
  const particle = (i, time) => {
    const { lw, lh, s } = layout;
    const depth = seed(i, 2), rate = 8 * (0.8 + depth * 1.2);
    const travel = seed(i, 1) * 680 + (time + 8) * rate;
    const identity = i + Math.floor(travel / 680) * 4099;
    const y = -70 + mod(travel, 680);
    const x = -50 + mod(seed(i, 3) * 1060 + (time + 8) * rate * 0.17, 1060);
    const fade = clamp01(Math.min((y + 70) / 80, (610 - y) / 80));
    const column = seed(identity, 6) < 0.01 ? 4 : Math.floor(seed(identity, 21) * 4);
    const row = Math.round(depth * 7);
    const distance = Math.max(Math.hypot(x - 480, y - 290) - 60, 0) / 370;
    const quiet = distance < 0.6 ? 0.58 + (0.23 - 0.58) * distance / 0.6 : 0.23 * Math.max(1 - distance, 0) / 0.4;
    return {
      x: x / 960 * lw * s,
      y: y / 540 * lh * s,
      side: 72 * (0.22 + depth * 0.36) * 64 / 56 * Math.max(Math.min(lw / 960, lh / 540), 0.65) * s,
      alpha: (0.13 + depth * 0.24) * fade * (1 - quiet),
      column,
      row,
    };
  };
  const rainColor = new Float64Array(3);
  const drawRain = (time) => {
    const { W, H } = layout;
    for (let i = 0; i < 50; i++) {
      const p = particle(i, time);
      if (p.alpha <= 0) continue;
      // The stationary field the menu samples across the viewport
      // (seconds 9.04, phase .363), at the outline's middle, at 35% of
      // its saturation.
      paint(p.x / W * 96 - 48, p.y / H * 96 - 48, 9.04, 0.363, rainColor, 0);
      const l = 0.2126 * rainColor[0] + 0.7152 * rainColor[1] + 0.0722 * rainColor[2];
      const c = rainColor.map((v) => Math.round(mix(l, v, 0.35)));
      ctx.save();
      ctx.translate(p.x, p.y);
      ctx.scale(p.side / 64, p.side / 64);
      ctx.translate(-32, -32);
      ctx.globalAlpha = p.alpha;
      ctx.strokeStyle = `rgb(${c[0]},${c[1]},${c[2]})`;
      ctx.lineWidth = D.weights[p.row];
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      if (p.column === 4) {
        ctx.translate(0, -7);
        ctx.stroke(onigiri);
      } else {
        ctx.translate(32, 32);
        ctx.rotate(p.column * Math.PI / 2);
        ctx.translate(-32, -32);
        ctx.stroke(arrow);
      }
      ctx.restore();
    }
  };

  // The bar: the theme's line, the done part painted as the focused
  // button's outline is (the field a square as wide as the viewport,
  // centered on it, evolving fast).
  const drawBar = (time) => {
    const { bar: b, W, H, dpr } = layout;
    ctx.fillStyle = color("--line", "#3a3542");
    ctx.fillRect(b.x, b.y - 1, b.w, 2);
    const done = Math.max(0.04, shown) * b.w;
    const cols = Math.max(1, Math.ceil(done * dpr / 2));
    if (!bar.image || bar.image.width !== cols) {
      bar.c.width = cols;
      bar.c.height = 1;
      bar.image = bar.c.getContext("2d").createImageData(cols, 1);
    }
    const [seconds, phase] = activeClock(time), data = bar.image.data;
    for (let i = 0, at = 0; i < cols; i++, at += 4) {
      paint(((b.x + (i + 0.5) / cols * done) / W - 0.5) * 128, (b.y - H / 2) / W * 128, seconds, phase, data, at);
      data[at + 3] = 255;
    }
    bar.c.getContext("2d").putImageData(bar.image, 0, 0);
    ctx.imageSmoothingEnabled = true;
    ctx.drawImage(bar.c, b.x, b.y - 3, done, 6);
  };

  // What a frame costs (`__DDI_ART.cost`, for the verify skill).
  const cost = { frames: 0, last: 0, max: 0, total: 0 };
  const frame = () => {
    raf = 0;
    if (!layout || !ctx) return;
    const began = performance.now();
    const time = reduced ? 0 : now();
    step(began);
    const { W, H, dpr } = layout;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
    drawRain(time);
    drawTitle(time);
    drawBar(time);
    cost.last = performance.now() - began;
    cost.max = Math.max(cost.max, cost.last);
    cost.total += cost.last;
    cost.frames++;
    if ((!reduced || (finishing && shown < 1)) && !document.hidden) raf = requestAnimationFrame(frame);
  };
  const wake = () => {
    if (!raf && layout && ctx) raf = requestAnimationFrame(frame);
  };
  document.addEventListener("visibilitychange", wake);

  window.__DDI_ART = {
    cost,
    // The bar's fill as drawn and as reported, 0 to 1, for the verify
    // skill.
    get shown() {
      return shown;
    },
    get target() {
      return target;
    },
    // `l`: the window's size and pixel ratio, the title's box and the
    // bar's rect in CSS pixels, the menu's layout size and scale.
    layout(l) {
      layout = l;
      canvas = l.canvas;
      canvas.width = Math.ceil(l.W * l.dpr);
      canvas.height = Math.ceil(l.H * l.dpr);
      ctx = canvas.getContext("2d");
      layoutTitle();
      wake();
    },
    // Reduced motion: one frame, at time zero, as the game holds it.
    still(yes) {
      reduced = yes;
      wake();
    },
    progress(ratio) {
      target = Math.max(target, ratio);
      wake();
    },
    // The game is ready: the fill's rest over FINISH_MS, the ms until
    // it is full (none under reduced motion).
    finish() {
      finishing = 1 - shown;
      wake();
      return reduced ? 0 : FINISH_MS;
    },
    stop() {
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
      layout = null;
      document.removeEventListener("visibilitychange", wake);
    },
  };
})();
