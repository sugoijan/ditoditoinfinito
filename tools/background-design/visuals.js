// The background study's candidates (docs/plans/phase-8-gaps.md, step 4):
// spectrum bars over a base, a kaleidoscope whose picture turns with the
// beat under still mirrors, a quiet abstract field with lines waving with the spectrum, and
// the start menu's outline rain with some outlines zooming with the bass
// and others waving with the treble. Each follows only the music (its analysis, `analysis.js`) and
// the beats its simfile gives (`timing.js`), never a play. WebGL2 draws
// the pictures and fields, a 2D canvas over it the lines and outlines.
"use strict";

(() => {
  // Quiet palettes, mostly dark and of few hues, and Vivid among them:
  // ground, low, mid, high.
  const PALETTES = {
    ink: { label: "Ink (blue-gray)", colors: ["#05020d", "#141a2b", "#4b5875", "#aab4c9"] },
    dusk: { label: "Dusk (violet, rose)", colors: ["#07050c", "#1c1530", "#5b4a7c", "#cfa7c6"] },
    moss: { label: "Moss (green)", colors: ["#040807", "#11211c", "#3c6855", "#9cc8b1"] },
    ember: { label: "Ember (warm)", colors: ["#0a0505", "#271311", "#7a3b2e", "#d79070"] },
    mono: { label: "Mono (gray)", colors: ["#050505", "#171717", "#555555", "#c9c9c9"] },
    vivid: { label: "Vivid (Infinito)", colors: ["#05020d", "#1a0f2e", "#6b5bff", "#31d5ff"], vivid: true },
  };

  // What may drive a motion: the music alone.
  const DRIVERS = [
    ["none", "Nothing"],
    ["loudness", "Loudness"],
    ["bass", "Bass bands"],
    ["mid", "Middle bands"],
    ["treble", "Treble bands"],
    ["onset", "Onsets"],
    ["brightness", "Brightness"],
    ["width", "Stereo width"],
    ["section", "Sections (slow loudness)"],
    ["beat", "Beat pulse"],
    ["measure", "Measure pulse"],
  ];

  const range = (id, label, min, max, step, value) => ({ id, label, kind: "range", min, max, step, value });
  const select = (id, label, options, value) => ({ id, label, kind: "select", options, value });
  const check = (id, label, value) => ({ id, label, kind: "check", value });
  const driver = (id, label, value) => select(id, label, DRIVERS, value);

  const BASES = [["ground", "Plain ground"], ["field", "Abstract field"], ["kaleido", "Kaleidoscope"], ["rain", "Outline rain"]];

  const CANDIDATES = [
    {
      id: "bars",
      label: "Spectrum bars",
      overlay: true,
      params: [
        select("base", "Base", BASES, "ground"),
        range("count", "Bars", 8, 64, 1, 24),
        // Across the whole screen, behind the field's band too.
        select("layout", "Layout", [["bottom", "Along the bottom"], ["mirror", "Mirrored, middle"], ["ring", "Ring"]], "bottom"),
        select("style", "Bars", [["segmented", "Segmented"], ["solid", "Solid"]], "segmented"),
        range("height", "Height", 0.05, 1, 0.01, 0.42),
        range("gap", "Gap between bars", 0, 0.8, 0.01, 0.3),
        range("segments", "Segments at full height", 4, 40, 1, 16),
        range("segmentGap", "Gap between segments", 0.05, 0.7, 0.01, 0.3),
        check("unlit", "Unlit segments, faint", false),
        check("peaks", "Peaks held, falling", true),
        range("attack", "Attack (s)", 0, 0.5, 0.005, 0.03),
        range("release", "Release (s)", 0, 1.5, 0.01, 0.28),
        select("color", "Color", [["high", "Palette high"], ["mid", "Palette middle"], ["gradient", "Middle to high, by band"], ["level", "Middle to high, by height"]], "level"),
        range("opacity", "Opacity", 0, 1, 0.01, 0.7),
      ],
    },
    {
      id: "kaleido",
      label: "Kaleidoscope",
      overlay: false,
      // The mirrors stay still; the picture under them moves, as a
      // kaleidoscope's object cell turns.
      params: [
        select("source", "Picture", [["background", "Song background"], ["banner", "Banner"], ["generated", "Generated (glass pieces)"]], "background"),
        select("mirrors", "Mirrors", [["three", "Three: triangles across the screen"], ["two", "Two: a rosette"]], "three"),
        range("cell", "Triangle side (screen heights)", 0.12, 1, 0.01, 0.32),
        range("radius", "Rosette radius (screen heights)", 0.3, 2, 0.01, 1),
        range("segments", "Rosette segments", 3, 12, 1, 6),
        check("rings", "Rosette mirrored outward in rings", true),
        range("piece", "Piece of the picture (its heights)", 0.05, 1, 0.01, 0.22),
        select("turn", "The picture turns", [["beat", "A step each beat"], ["measure", "A step each measure"], ["steady", "Steadily"], ["none", "Not at all"]], "beat"),
        range("step", "Step (degrees)", 0, 90, 1, 15),
        driver("growBy", "The piece grows with", "bass"),
        range("grow", "Growth", 0, 0.6, 0.01, 0.12),
        range("wander", "The piece wanders", 0, 1, 0.01, 0.3),
        driver("wanderBy", "Wandering follows", "section"),
        range("desaturate", "Desaturate", 0, 1, 0.01, 0.55),
        range("darken", "Darken", 0, 1, 0.01, 0.6),
        range("tint", "Palette tint", 0, 1, 0.01, 0.3),
        driver("pulseBy", "Brightens on", "beat"),
        range("pulse", "Brighten amount", 0, 1, 0.01, 0.12),
      ],
    },
    {
      id: "field",
      label: "Field and waving lines",
      overlay: false,
      params: [
        range("scale", "Field scale", 0.3, 5, 0.01, 1.6),
        range("contrast", "Field contrast", 0, 1, 0.01, 0.55),
        range("speed", "Field speed", 0, 1, 0.01, 0.1),
        driver("speedBy", "Speed follows", "section"),
        range("bright", "Field brightness", 0, 1, 0.01, 0.35),
        driver("brightBy", "Brightness follows", "loudness"),
        range("brightAmount", "Brightness amount", 0, 1, 0.01, 0.35),
        range("lines", "Lines", 0, 24, 1, 9),
        select("area", "Lines", [["full", "Across the screen"], ["sides", "Beside the field"]], "full"),
        select("wave", "Waves follow", [["bands", "Each line its bands"], ["waveform", "The waveform"]], "bands"),
        range("amplitude", "Amplitude (px)", 0, 120, 1, 26),
        range("cycles", "Waves across", 0.5, 10, 0.1, 3),
        range("lineSpeed", "Wave speed", 0, 4, 0.01, 0.6),
        range("lineWidth", "Line width", 0.3, 4, 0.1, 1.2),
        range("lineOpacity", "Line opacity", 0, 1, 0.01, 0.55),
        select("lineColor", "Line color", [["mid", "Palette middle"], ["high", "Palette high"], ["gradient", "Middle to high"]], "gradient"),
      ],
    },
    {
      id: "rain",
      label: "Outline rain",
      overlay: true,
      params: [
        select("base", "Base", BASES.filter(([v]) => v !== "rain"), "ground"),
        range("count", "Outlines", 10, 140, 1, 50),
        range("size", "Size", 0.4, 2.5, 0.01, 1),
        range("speed", "Fall speed", 0, 3, 0.01, 1),
        // A share of the outlines move, each chosen afresh as it comes in
        // at the top: some zoom, the rest wave.
        range("vibrating", "Moving share", 0, 1, 0.01, 0.35),
        range("zoomShare", "Of those, zooming (the rest wave)", 0, 1, 0.01, 0.5),
        driver("zoomBy", "Zoom follows (what stands out of the song)", "bass"),
        range("zoom", "Zoom (of the size)", 0, 1, 0.01, 0.3),
        driver("waveBy", "Waves follow (what stands out of the song)", "treble"),
        range("amplitude", "Waves (units of 64)", 0, 12, 0.1, 3),
        range("waves", "Waves round an outline", 1, 16, 1, 6),
        range("frequency", "Wave speed (Hz)", 0, 20, 0.1, 5),
        select("color", "Color", [["quiet", "Palette, quiet"], ["vivid", "Vivid at 35% (the menu's)"], ["white", "White"]], "quiet"),
        range("opacity", "Opacity", 0, 2.5, 0.01, 1),
        check("quietMiddle", "Quieter round the middle (the menu's)", true),
      ],
    },
  ];

  // Color helpers.
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
  const css = (c, a = 1) => `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${a})`;
  const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
  const clamp01 = (v) => Math.min(1, Math.max(0, v));
  const fract = (v) => v - Math.floor(v);
  const seed = (i, salt) => fract(Math.sin(i * 127.1 + salt * 311.7) * 43758.5453);
  const mod = (a, n) => a - Math.floor(a / n) * n;
  const VIVID = ["#31d5ff", "#6b5bff", "#c33dff", "#ff4eb9", "#ff883d", "#e3ec3e", "#48ef9b", "#31d5ff"].map(hex);
  const vivid = (t) => {
    const x = mod(t, 1) * (VIVID.length - 1);
    const i = Math.floor(x);
    return mix(VIVID[i], VIVID[i + 1], x - i);
  };
  const luma = (c) => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];

  // WebGL2 programs.
  // A triangle covering the viewport; attribute 0 is a buffer of one
  // value a corner, kept enabled (Firefox emulates a draw without it).
  const VERTEX = `#version 300 es
    layout(location = 0) in float corner;
    out vec2 uv;
    void main() {
      int id = int(corner);
      vec2 p = vec2(id & 1, id >> 1) * 2.0;
      uv = vec2(p.x, 1.0 - p.y);
      gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
    }`;
  const NOISE = `
    float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
    float noise(vec2 p) {
      vec2 i = floor(p), f = fract(p);
      vec2 u = f * f * (3.0 - 2.0 * f);
      return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
    }
    float fbm(vec2 p) {
      float v = 0.0, a = 0.5;
      for (int i = 0; i < 5; i++) { v += a * noise(p); p = p * 2.03 + vec2(17.0, 9.0); a *= 0.5; }
      return v;
    }`;
  const PICTURE = `#version 300 es
    precision highp float;
    in vec2 uv; out vec4 color;
    uniform sampler2D image; uniform vec4 fit; uniform float keep;
    void main() {
      vec3 c = texture(image, uv * fit.xy + fit.zw).rgb;
      color = vec4(c * keep, 1.0);
    }`;
  const FIELD = `#version 300 es
    precision highp float;
    in vec2 uv; out vec4 color;
    uniform float time, scale, contrast, bright, aspect;
    uniform vec3 c0, c1, c2, c3;
    ${NOISE}
    void main() {
      vec2 p = (uv - 0.5) * vec2(aspect, 1.0) * scale;
      vec2 q = vec2(fbm(p + time * 0.15), fbm(p + vec2(5.2, 1.3) - time * 0.11));
      float f = fbm(p + 2.2 * q + time * 0.05);
      f = clamp(0.5 + (f - 0.5) * (0.6 + contrast * 2.4), 0.0, 1.0);
      vec3 c = f < 0.5 ? mix(c0, c1, f * 2.0) : mix(c1, c2, (f - 0.5) * 2.0);
      c = mix(c, c3, smoothstep(0.82, 1.0, f) * 0.25);
      color = vec4(mix(c0, c, clamp(bright * 1.6, 0.0, 1.0)), 1.0);
    }`;
  // A kaleidoscope: the screen folded by still mirrors into one cell (a
  // triangle of three mirrors tiling the screen, or a rosette's wedge of
  // two), and that cell showing a small piece of the picture, which
  // turns, grows and wanders under them.
  const KALEIDO = `#version 300 es
    precision highp float;
    in vec2 uv; out vec4 color;
    uniform sampler2D image; uniform bool generated, three, rings;
    uniform float segments, cell, aspect, imageAspect, piece, turn, desaturate, darken, tint, pulse;
    uniform vec2 center; uniform vec3 tintColor, ground, c1, c2, c3;
    ${NOISE}
    const float H = 0.8660254;
    // q reflected into the triangle (0, 0), (1, 0), (0.5, H) by its
    // mirrors, a wall it is outside of at a time.
    vec2 triangle(vec2 q) {
      for (int i = 0; i < 64; i++) {
        bool inside = true;
        if (q.y < 0.0) { q.y = -q.y; inside = false; }
        vec2 n = vec2(H, -0.5);
        float d = dot(q, n);
        if (d < 0.0) { q -= 2.0 * d * n; inside = false; }
        n = vec2(-H, -0.5);
        d = dot(q - vec2(1.0, 0.0), n);
        if (d < 0.0) { q -= 2.0 * d * n; inside = false; }
        if (inside) break;
      }
      return q;
    }
    // Pieces of glass in the palette, for a song with no picture.
    vec3 glass(vec2 p) {
      vec2 g = p * 14.0, i = floor(g), f = fract(g), id = vec2(0.0);
      float best = 9.0, second = 9.0;
      for (int y = -1; y <= 1; y++) for (int x = -1; x <= 1; x++) {
        vec2 o = vec2(float(x), float(y));
        float d = length(f - o - vec2(hash(i + o), hash(i + o + 19.7)));
        if (d < best) { second = best; best = d; id = i + o; }
        else if (d < second) second = d;
      }
      float h = hash(id + 3.1);
      vec3 c = h < 0.45 ? c1 : h < 0.85 ? c2 : c3;
      c *= 0.7 + 0.6 * hash(id + 7.3);
      return c * mix(0.2, 1.0, smoothstep(0.0, 0.08, second - best));
    }
    // The picture at p, in its heights; mirrored past its edges.
    vec3 source(vec2 p) {
      if (generated) return glass(p);
      vec2 m = abs(mod(vec2(p.x / imageAspect, p.y), 2.0) - 1.0);
      return texture(image, 1.0 - m).rgb;
    }
    void main() {
      vec2 p = (uv - 0.5) * vec2(aspect, 1.0) / cell;  // in triangle sides, or rosette radii
      vec2 local;
      if (three) {
        // A vertex in the middle, six triangles round it.
        local = triangle(p) - vec2(0.5, H / 3.0);
      } else {
        float r = length(p), a = atan(p.y, p.x);
        float wedge = 6.2831853 / segments;
        a = mod(a, wedge);
        a = min(a, wedge - a);
        if (rings) r = 1.0 - abs(mod(r, 2.0) - 1.0);
        local = vec2(cos(a), sin(a)) * r - 0.6 * vec2(cos(wedge * 0.25), sin(wedge * 0.25));
      }
      float cs = cos(turn), sn = sin(turn);
      vec3 c = source(center + mat2(cs, sn, -sn, cs) * local * piece);
      float l = dot(c, vec3(0.2126, 0.7152, 0.0722));
      c = mix(c, vec3(l), desaturate);
      c = mix(c, tintColor * (0.3 + l), tint * 0.6);
      c *= (1.0 - darken) * (1.0 + pulse);
      color = vec4(mix(ground, c, 0.97), 1.0);
    }`;

  function program(gl, fragment) {
    const make = (type, src) => {
      const s = gl.createShader(type);
      gl.shaderSource(s, src);
      gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
      return s;
    };
    const p = gl.createProgram();
    gl.attachShader(p, make(gl.VERTEX_SHADER, VERTEX));
    gl.attachShader(p, make(gl.FRAGMENT_SHADER, fragment));
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
    const u = new Proxy({}, { get: (_, name) => gl.getUniformLocation(p, name) });
    return { p, u };
  }

  // The rain's outlines as points round them, with their normals, in the
  // atlas's 64-unit square.
  function outline(d, n = 120) {
    // Measured in the document (Firefox measures no detached path).
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("width", "0");
    svg.setAttribute("height", "0");
    svg.style.position = "absolute";
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", d);
    svg.append(path);
    document.body.append(svg);
    const length = path.getTotalLength();
    const points = [];
    for (let i = 0; i < n; i++) {
      const p = path.getPointAtLength((i / n) * length);
      points.push([p.x, p.y]);
    }
    svg.remove();
    return points.map((p, i) => {
      const a = points[(i + n - 1) % n], b = points[(i + 1) % n];
      const tx = b[0] - a[0], ty = b[1] - a[1];
      const l = Math.hypot(tx, ty) || 1;
      return { x: p[0], y: p[1], nx: ty / l, ny: -tx / l, s: i / n };
    });
  }

  /** The study's renderer over a WebGL2 canvas and a 2D one above it. */
  function createRenderer(glCanvas, overlay) {
    const gl = glCanvas.getContext("webgl2", { premultipliedAlpha: false, preserveDrawingBuffer: true });
    if (!gl) throw new Error("WebGL2 is not available");
    const ctx = overlay.getContext("2d");
    const programs = { picture: program(gl, PICTURE), field: program(gl, FIELD), kaleido: program(gl, KALEIDO) };
    const corners = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, corners);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 1, 2]), gl.STATIC_DRAW);
    gl.bindVertexArray(gl.createVertexArray());
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 1, gl.FLOAT, false, 0, 0);
    const textures = {};
    const sizes = {};
    const data = window.__DDI_ART_DATA || {};
    const shapes = {
      arrow: data.arrow ? outline(data.arrow) : null,
      onigiri: data.onigiri ? outline(data.onigiri) : null,
    };
    const weights = data.weights || [4, 3.3, 2.8, 2.4, 2.1, 1.9, 1.7, 1.5];
    // Each candidate's smoothed bands (attack and release), by key.
    const smoothed = {};
    // Signals followed (`follow`), and the bars' peaks.
    const followed = {};
    let peaks = null;

    function setImage(name, bitmap) {
      if (!bitmap) {
        delete sizes[name];
        return;
      }
      const t = textures[name] || (textures[name] = gl.createTexture());
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, bitmap);
      gl.generateMipmap(gl.TEXTURE_2D);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      sizes[name] = [bitmap.width, bitmap.height];
    }

    // Drawing within `rect` (CSS pixels) of the canvases, `scale` device
    // pixels a CSS pixel.
    function viewport(rect, scale) {
      const [x, y, w, h] = rect.map((v) => Math.round(v * scale));
      gl.viewport(x, glCanvas.height - y - h, w, h);
      gl.scissor(x, glCanvas.height - y - h, w, h);
    }

    function drawPicture(name, rect, scale, keep) {
      if (!sizes[name]) return false;
      const { p, u } = programs.picture;
      gl.useProgram(p);
      viewport(rect, scale);
      const [iw, ih] = sizes[name];
      const ra = rect[2] / rect[3], ia = iw / ih;
      const sx = ra > ia ? 1 : ra / ia, sy = ra > ia ? ia / ra : 1;
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, textures[name]);
      gl.uniform1i(u.image, 0);
      gl.uniform4f(u.fit, sx, sy, (1 - sx) / 2, (1 - sy) / 2);
      gl.uniform1f(u.keep, keep);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      return true;
    }

    function drawGround(rect, scale, palette) {
      viewport(rect, scale);
      const g = hex(palette.colors[0]);
      gl.clearColor(g[0], g[1], g[2], 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }

    function drawField(rect, scale, f, p, palette) {
      const { p: prog, u } = programs.field;
      gl.useProgram(prog);
      viewport(rect, scale);
      const cs = palette.colors.map(hex);
      gl.uniform3fv(u.c0, cs[0]);
      gl.uniform3fv(u.c1, cs[1]);
      gl.uniform3fv(u.c2, cs[2]);
      gl.uniform3fv(u.c3, cs[3]);
      gl.uniform1f(u.time, f.clock("field", p.speed, p.speedBy));
      gl.uniform1f(u.scale, p.scale);
      gl.uniform1f(u.contrast, p.contrast);
      gl.uniform1f(u.bright, p.bright * (1 - p.brightAmount) + p.bright * p.brightAmount * 2 * f.signal(p.brightBy));
      gl.uniform1f(u.aspect, rect[2] / rect[3]);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }

    function drawKaleido(rect, scale, f, p, palette, images) {
      const { p: prog, u } = programs.kaleido;
      gl.useProgram(prog);
      viewport(rect, scale);
      const name = p.source === "banner" ? images.banner : p.source === "background" ? images.background : null;
      const has = name && sizes[name];
      gl.activeTexture(gl.TEXTURE0);
      if (has) gl.bindTexture(gl.TEXTURE_2D, textures[name]);
      gl.uniform1i(u.image, 0);
      gl.uniform1i(u.generated, has ? 0 : 1);
      gl.uniform1i(u.three, p.mirrors === "two" ? 0 : 1);
      gl.uniform1i(u.rings, p.rings ? 1 : 0);
      gl.uniform1f(u.segments, p.segments);
      gl.uniform1f(u.cell, p.mirrors === "two" ? p.radius : p.cell);
      gl.uniform1f(u.aspect, rect[2] / rect[3]);
      // Every motion is the picture's; the mirrors stay where they are.
      const step = (p.step * Math.PI) / 180;
      const eased = (x) => Math.floor(x) + (1 - Math.pow(1 - fract(x), 3));
      const turn = p.turn === "beat" ? eased(f.beat) * step
        : p.turn === "measure" ? eased(f.measure + f.measurePhase) * step
        : p.turn === "steady" ? f.t * step * 0.5
        : 0;
      gl.uniform1f(u.turn, turn);
      const piece = p.piece * (1 + p.grow * follow("kaleido.grow", f.signal(p.growBy), f.dt));
      gl.uniform1f(u.piece, piece);
      // The piece wanders over the picture, kept inside it (a cell's
      // points lie within 0.62 of its middle).
      const ia = has ? sizes[name][0] / sizes[name][1] : 1;
      gl.uniform1f(u.imageAspect, ia);
      const w = f.clock("kaleido", p.wander * 0.25, p.wanderBy);
      const m = 0.62 * piece;
      const along = (lo, hi, s) => (lo < hi ? lo + (hi - lo) * (0.5 + 0.5 * s) : (lo + hi) / 2);
      if (has) gl.uniform2f(u.center, along(m, ia - m, Math.sin(w * 1.3)), along(m, 1 - m, Math.cos(w * 0.9)));
      else gl.uniform2f(u.center, 4 + 3 * Math.sin(w * 1.3), 4 + 3 * Math.cos(w * 0.9));
      gl.uniform1f(u.desaturate, p.desaturate);
      gl.uniform1f(u.darken, p.darken);
      gl.uniform1f(u.tint, p.tint);
      gl.uniform1f(u.pulse, p.pulse * f.signal(p.pulseBy));
      const cs = palette.colors.map(hex);
      gl.uniform3fv(u.tintColor, palette.vivid ? vivid(f.t * 0.02) : cs[3]);
      gl.uniform3fv(u.ground, cs[0]);
      gl.uniform3fv(u.c1, mix(cs[1], cs[2], 0.5));
      gl.uniform3fv(u.c2, cs[2]);
      gl.uniform3fv(u.c3, cs[3]);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }

    // `v` followed with `attack` and `release` seconds, under `key`.
    function follow(key, v, dt, attack = 0.06, release = 0.35) {
      const s = followed[key] ?? v;
      const k = v > s ? attack : release;
      const d = Math.max(0, Math.min(0.1, dt));
      return (followed[key] = s + (v - s) * (1 - Math.exp(-d / k)));
    }

    // `key`'s bands smoothed toward the frame's with `attack` and
    // `release` seconds.
    function bandsFor(key, f, attack, release) {
      const now = f.bands;
      let s = smoothed[key];
      if (!s || s.length !== now.length) s = smoothed[key] = Float32Array.from(now);
      const dt = Math.max(0, Math.min(0.1, f.dt));
      for (let i = 0; i < now.length; i++) {
        const k = now[i] > s[i] ? attack : release;
        s[i] += (now[i] - s[i]) * (k <= 0 ? 1 : 1 - Math.exp(-dt / k));
      }
      return s;
    }

    // The field's area within `rect`: StepMania's four lanes and their
    // band, in the middle, as wide as a play's field on this shape.
    const fieldArea = (rect) => {
      const w = rect[3] * 0.62;
      return [rect[0] + (rect[2] - w) / 2, w];
    };

    function drawBars(rect, f, p, palette) {
      const bands = bandsFor("bars", f, p.attack, p.release);
      const cs = palette.colors.map(hex);
      const n = Math.round(p.count);
      // Bars from the bands, by mean.
      const value = (i) => {
        const a = Math.floor((i / n) * bands.length), b = Math.max(a + 1, Math.floor(((i + 1) / n) * bands.length));
        let v = 0;
        for (let k = a; k < b; k++) v += bands[k];
        return v / (b - a);
      };
      // `level` is how high up the bar the color is, 0 to 1.
      const color = (i, level) => {
        if (palette.vivid && p.color !== "mid") return vivid(i / n * 0.6 + f.t * 0.03);
        switch (p.color) {
          case "high": return cs[3];
          case "mid": return cs[2];
          case "level": return mix(cs[2], cs[3], level);
          default: return mix(cs[2], cs[3], i / Math.max(1, n - 1));
        }
      };
      // Peaks: each bar's highest, held half a second, then falling.
      const values = Array.from({ length: n }, (_, i) => value(i));
      if (!peaks || peaks.v.length !== n) peaks = { v: Float32Array.from(values), hold: new Float32Array(n) };
      const dt = Math.max(0, Math.min(0.1, f.dt));
      for (let i = 0; i < n; i++) {
        if (values[i] >= peaks.v[i]) {
          peaks.v[i] = values[i];
          peaks.hold[i] = 0.5;
        } else if (peaks.hold[i] > 0) peaks.hold[i] -= dt;
        else peaks.v[i] = Math.max(values[i], peaks.v[i] - dt * 0.6);
      }
      const segmented = p.style !== "solid";
      const cells = Math.round(p.segments);
      // A bar from (x, y) at angle `a` (0 grows up), `bw` wide, `full`
      // long at its loudest.
      const bar = (x, y, a, bw, full, i) => {
        const v = values[i], peak = peaks.v[i];
        ctx.save();
        ctx.translate(x, y);
        ctx.rotate(a);
        if (segmented) {
          const pitch = full / cells, ch = pitch * (1 - p.segmentGap), g = (pitch - ch) / 2;
          const lit = Math.min(cells, Math.floor(v * cells + 0.35));
          for (let j = 0; j < cells; j++) {
            if (j >= lit && !p.unlit) break;
            ctx.globalAlpha = p.opacity * (j < lit ? 1 : 0.1);
            ctx.fillStyle = css(color(i, j / Math.max(1, cells - 1)));
            ctx.fillRect(-bw / 2, -(j * pitch + g + ch), bw, ch);
          }
          const top = Math.min(cells, Math.floor(peak * cells + 0.35)) - 1;
          if (p.peaks && top >= lit) {
            ctx.globalAlpha = p.opacity;
            ctx.fillStyle = css(color(i, top / Math.max(1, cells - 1)));
            ctx.fillRect(-bw / 2, -(top * pitch + g + ch), bw, ch);
          }
        } else {
          ctx.globalAlpha = p.opacity;
          ctx.fillStyle = css(color(i, v));
          ctx.fillRect(-bw / 2, -v * full, bw, v * full);
          if (p.peaks && peak > v + 0.02) {
            const t = Math.max(1.5, full * 0.012);
            ctx.fillStyle = css(color(i, peak));
            ctx.fillRect(-bw / 2, -peak * full - t, bw, t);
          }
        }
        ctx.restore();
      };
      ctx.save();
      ctx.beginPath();
      ctx.rect(...rect);
      ctx.clip();
      const [x0, y0, w, h] = rect;
      if (p.layout === "ring") {
        const cx = x0 + w / 2, cy = y0 + h / 2, r0 = h * 0.36;
        const bw = Math.max(1, ((2 * Math.PI * r0) / n) * (1 - p.gap));
        for (let i = 0; i < n; i++) {
          const a = (i / n) * Math.PI * 2 - Math.PI / 2;
          bar(cx + Math.cos(a) * r0, cy + Math.sin(a) * r0, a + Math.PI / 2, bw, h * p.height * 0.5, i);
        }
      } else {
        const bw = w / n;
        for (let i = 0; i < n; i++) {
          const x = x0 + (i + 0.5) * bw;
          if (p.layout === "mirror") {
            bar(x, y0 + h / 2, 0, bw * (1 - p.gap), (h * p.height) / 2, i);
            bar(x, y0 + h / 2, Math.PI, bw * (1 - p.gap), (h * p.height) / 2, i);
          } else bar(x, y0 + h, 0, bw * (1 - p.gap), h * p.height, i);
        }
      }
      ctx.restore();
    }

    function drawLines(rect, f, p, palette) {
      const n = Math.round(p.lines);
      if (!n) return;
      const bands = bandsFor("lines", f, 0.04, 0.3);
      const cs = palette.colors.map(hex);
      const [x0, y0, w, h] = rect;
      const [fx, fw] = fieldArea(rect);
      ctx.save();
      ctx.beginPath();
      ctx.rect(...rect);
      if (p.area === "sides") {
        ctx.rect(fx + fw, y0, -fw, h);
      }
      ctx.clip("evenodd");
      ctx.lineWidth = p.lineWidth;
      ctx.lineJoin = "round";
      const steps = 160;
      for (let l = 0; l < n; l++) {
        const y = y0 + ((l + 1) / (n + 1)) * h;
        const c = palette.vivid ? vivid(l / n * 0.5 + f.t * 0.02)
          : p.lineColor === "mid" ? cs[2] : p.lineColor === "high" ? cs[3] : mix(cs[2], cs[3], l / Math.max(1, n - 1));
        ctx.strokeStyle = css(c, p.lineOpacity);
        ctx.beginPath();
        for (let s = 0; s <= steps; s++) {
          const u = s / steps;
          let d = 0;
          if (p.wave === "waveform") {
            d = f.wave(u, l) * 1.6;
          } else {
            // This line's share of the bands, each a harmonic of the wave.
            const per = bands.length / n;
            for (let k = 0; k < 4; k++) {
              const b = Math.min(bands.length - 1, Math.floor(l * per + (k * per) / 4));
              d += bands[b] * Math.sin(2 * Math.PI * (u * p.cycles * (k + 1)) + f.t * p.lineSpeed * (k + 1) * 2 + l * 1.7 + k) / (k + 1);
            }
            d *= 0.7;
          }
          const taper = Math.sin(Math.PI * u);
          const x = x0 + u * w, yy = y + d * p.amplitude * taper;
          if (s) ctx.lineTo(x, yy);
          else ctx.moveTo(x, yy);
        }
        ctx.stroke();
      }
      ctx.restore();
    }

    function drawRain(rect, f, p, palette) {
      const cs = palette.colors.map(hex);
      const [x0, y0, w, h] = rect;
      const units = h / 540;
      const time = f.t * p.speed;
      ctx.save();
      ctx.beginPath();
      ctx.rect(...rect);
      ctx.clip();
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      // The two motions' levels, shared by the outlines making them.
      const zoomed = 1 + p.zoom * follow("rain.zoom", f.lift(p.zoomBy), f.dt, 0.02, 0.18);
      const waved = p.amplitude * follow("rain.wave", f.lift(p.waveBy), f.dt, 0.02, 0.25);
      const n = Math.round(p.count);
      for (let i = 0; i < n; i++) {
        // crates/theme/src/rain.rs, as site/boot-art.js has it, over the
        // menu's 960 × 540 layout scaled to the rect's height.
        const depth = seed(i, 2), rate = 8 * (0.8 + depth * 1.2);
        const travel = seed(i, 1) * 680 + (time + 8) * rate;
        const identity = i + Math.floor(travel / 680) * 4099;
        const y = -70 + mod(travel, 680);
        const x = -50 + mod(seed(i, 3) * 1060 + (time + 8) * rate * 0.17, 1060);
        const fade = clamp01(Math.min((y + 70) / 80, (610 - y) / 80));
        const column = seed(identity, 6) < 0.01 ? 4 : Math.floor(seed(identity, 21) * 4);
        const row = Math.round(depth * 7);
        const distance = Math.max(Math.hypot(x - 480, y - 290) - 60, 0) / 370;
        const quiet = !p.quietMiddle ? 0
          : distance < 0.6 ? 0.58 + ((0.23 - 0.58) * distance) / 0.6
          : (0.23 * Math.max(1 - distance, 0)) / 0.4;
        const alpha = Math.min(1, (0.13 + depth * 0.24) * fade * (1 - quiet) * p.opacity);
        if (alpha <= 0) continue;
        // Chosen as it comes in: moving or not, zooming or waving.
        const moving = seed(identity, 9) < p.vibrating;
        const zooming = moving && seed(identity, 13) < p.zoomShare;
        const z = zooming ? zoomed : 1;
        const side = 72 * (0.22 + depth * 0.36) * (64 / 56) * units * p.size * z;
        const px = x0 + (x / 960) * w, py = y0 + (y / 540) * h;
        let c;
        if (p.color === "white") c = [0.95, 0.94, 0.92];
        else if (p.color === "vivid" || palette.vivid) {
          const v = vivid(x / 960 * 0.5 + y / 540 * 0.3 + f.t * 0.03);
          c = mix([luma(v), luma(v), luma(v)], v, 0.35);
        } else c = mix(cs[2], cs[3], depth);
        const shape = column === 4 ? shapes.onigiri : shapes.arrow;
        if (!shape) continue;
        const amp = moving && !zooming ? waved : 0;
        const phase = f.t * 2 * Math.PI * p.frequency * (0.7 + seed(i, 11) * 0.6);
        ctx.save();
        ctx.translate(px, py);
        ctx.scale(side / 64, side / 64);
        ctx.translate(-32, -32);
        if (column !== 4) {
          ctx.translate(32, 32);
          ctx.rotate((column * Math.PI) / 2);
          ctx.translate(-32, -32);
        } else ctx.translate(0, -7);
        ctx.globalAlpha = alpha;
        ctx.strokeStyle = css(c);
        ctx.lineWidth = weights[row] / z;
        ctx.beginPath();
        shape.forEach((q, k) => {
          const d = amp * Math.sin(2 * Math.PI * p.waves * q.s + phase);
          const qx = q.x + q.nx * d, qy = q.y + q.ny * d;
          if (k) ctx.lineTo(qx, qy);
          else ctx.moveTo(qx, qy);
        });
        ctx.closePath();
        ctx.stroke();
        ctx.restore();
      }
      ctx.restore();
    }

    // The field drawn over a background: its band and four receptors.
    function drawFieldShape(rect, filter) {
      const [fx, fw] = fieldArea(rect);
      ctx.save();
      ctx.fillStyle = `rgba(0,0,0,${filter})`;
      ctx.fillRect(fx, rect[1], fw, rect[3]);
      if (shapes.arrow) {
        const size = fw / 4.4;
        const path = new Path2D(window.__DDI_ART_DATA.arrow);
        ctx.strokeStyle = "rgba(242,238,233,0.75)";
        ctx.lineWidth = 2.4;
        // Left, Down, Up, Right: the up-pointing outline turned.
        [-Math.PI / 2, Math.PI, 0, Math.PI / 2].forEach((turn, i) => {
          ctx.save();
          ctx.translate(fx + fw * (i + 0.5) / 4, rect[1] + rect[3] * 0.16);
          ctx.scale(size / 64, size / 64);
          ctx.rotate(turn);
          ctx.translate(-32, -32);
          ctx.stroke(path);
          ctx.restore();
        });
      }
      ctx.restore();
    }

    function drawBase(base, rect, scale, f, params, palette, images) {
      switch (base) {
        case "field": drawField(rect, scale, f, params.field, palette); return;
        case "kaleido": drawKaleido(rect, scale, f, params.kaleido, palette, images); return;
        default: drawGround(rect, scale, palette);
      }
    }

    /**
     * Draws `candidate` (an id of CANDIDATES) in `rect`: `f` the frame's
     * music (`t`, `dt`, `beat`, `measure`, `measurePhase`, `bands`,
     * `signal(name)`, `lift(name)`, `clock(key, speed, driver)`,
     * `wave(u, line)`),
     * `params` every candidate's values, `view` the study's options.
     */
    function draw(candidate, rect, scale, f, params, view) {
      const palette = PALETTES[view.palette] || PALETTES.ink;
      const images = { background: "background", banner: "banner" };
      gl.enable(gl.SCISSOR_TEST);
      const over = view.overPicture && sizes.background;
      const p = params[candidate];
      if (candidate === "kaleido") {
        drawKaleido(rect, scale, f, p, palette, images);
      } else if (candidate === "field") {
        if (over) drawPicture("background", rect, scale, view.pictureBrightness);
        else drawField(rect, scale, f, p, palette);
      } else if (over) {
        drawPicture("background", rect, scale, view.pictureBrightness);
      } else {
        drawBase(p.base, rect, scale, f, params, palette, images);
      }
      if (view.overPicture && !sizes.background) {
        // No picture: say so on the rect.
        ctx.save();
        ctx.fillStyle = "rgba(242,238,233,0.6)";
        ctx.font = "12px system-ui";
        ctx.fillText("no song background", rect[0] + 10, rect[1] + rect[3] - 10);
        ctx.restore();
      }
      if (candidate === "bars") {
        if (!over && p.base === "rain") drawRain(rect, f, params.rain, palette);
        drawBars(rect, f, p, palette);
      } else if (candidate === "field") {
        drawLines(rect, f, p, palette);
      } else if (candidate === "rain") {
        drawRain(rect, f, p, palette);
      }
      if (view.showField) drawFieldShape(rect, view.fieldFilter);
    }

    function resize(width, height, ratio) {
      for (const c of [glCanvas, overlay]) {
        c.width = Math.max(1, Math.round(width * ratio));
        c.height = Math.max(1, Math.round(height * ratio));
      }
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    }

    function clear() {
      ctx.clearRect(0, 0, overlay.width, overlay.height);
    }

    return { setImage, draw, resize, clear, get hasBackground() { return !!sizes.background; } };
  }

  window.BgVisuals = { PALETTES, DRIVERS, CANDIDATES, createRenderer };
})();
