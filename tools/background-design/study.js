// The generative backgrounds study (docs/plans/phase-8-gaps.md, step 4):
// a bundled song (or files picked) analyzed in the page, played along,
// each candidate drawn from the music alone with every parameter live,
// the analysis traced below, and the maintainer's choices exported as
// `approved.json`.
"use strict";

(() => {
  const { PALETTES, CANDIDATES, createRenderer } = window.BgVisuals;
  const { analyze, HOP } = window.BgAnalysis;
  const Timing = window.BgTiming;
  const KEY = "ddi.background-study.v1";
  const VERSION = 1;
  const SONGS = "../../assets/songs/";

  // The bundled songs (assets/songs/, each with its PROVENANCE.toml).
  const BUNDLED = [
    { id: "beatucada", title: "Beatucada", music: "Beatucada.opus", simfile: "Beatucada.ssc", background: "Background.png", banner: "Banner.png" },
    { id: "sweeteners", title: "Sweeteners", music: "Sweeteners.opus", simfile: "Sweeteners.ssc", background: "background.jpg", banner: "banner.jpg" },
    { id: "some-things-must", title: "Some Things Must (OutFox Edit)", music: "Some Things Must (OutFox Edit).opus", simfile: "Some Things Must (OutFox Edit).ssc", background: "sevish-bg.png", banner: "sevish-bn.png" },
    { id: "into-my-dream", title: "Into My Dream (no background)", music: "Into My Dream.opus", simfile: "Into My Dream.ssc", background: null, banner: "Into My Dream-bn.png" },
  ];

  const CHOOSING = [
    ["player", "The player's choice, under Options"],
    ["song", "One the game picks for each song"],
    ["fixed", "Always the same one"],
  ];

  const $ = (id) => document.getElementById(id);
  const defaults = () => ({
    version: VERSION,
    song: "beatucada",
    view: { mode: "single", candidate: "bars", palette: "ink", overPicture: false, pictureBrightness: 0.4, showField: true, fieldFilter: 0.4, reduced: false, resolution: 1 },
    params: Object.fromEntries(CANDIDATES.map((c) => [c.id, Object.fromEntries(c.params.map((p) => [p.id, p.value]))])),
    approval: {
      choose: "player",
      candidates: Object.fromEntries(CANDIDATES.map((c) => [c.id, { keep: false, overPicture: false, note: "" }])),
    },
  });

  // Only known, bounded values of `input` over `base` (stored or imported).
  function sanitize(input) {
    const out = defaults();
    if (!input || typeof input !== "object") return out;
    if (BUNDLED.some((s) => s.id === input.song)) out.song = input.song;
    const v = input.view || {};
    if (["single", "grid"].includes(v.mode)) out.view.mode = v.mode;
    if (CANDIDATES.some((c) => c.id === v.candidate)) out.view.candidate = v.candidate;
    if (PALETTES[v.palette]) out.view.palette = v.palette;
    for (const k of ["overPicture", "showField", "reduced"]) if (typeof v[k] === "boolean") out.view[k] = v[k];
    if (Number.isFinite(v.fieldFilter)) out.view.fieldFilter = Math.min(1, Math.max(0, v.fieldFilter));
    if (Number.isFinite(v.pictureBrightness)) out.view.pictureBrightness = Math.min(1, Math.max(0.1, v.pictureBrightness));
    if ([0.5, 1].includes(v.resolution)) out.view.resolution = v.resolution;
    for (const c of CANDIDATES) {
      const given = (input.params || {})[c.id] || {};
      for (const p of c.params) {
        const g = given[p.id];
        if (p.kind === "range" && Number.isFinite(g)) out.params[c.id][p.id] = Math.min(p.max, Math.max(p.min, g));
        if (p.kind === "select" && p.options.some(([o]) => o === g)) out.params[c.id][p.id] = g;
        if (p.kind === "check" && typeof g === "boolean") out.params[c.id][p.id] = g;
      }
      const a = ((input.approval || {}).candidates || {})[c.id] || {};
      const o = out.approval.candidates[c.id];
      if (typeof a.keep === "boolean") o.keep = a.keep;
      if (typeof a.overPicture === "boolean") o.overPicture = a.overPicture;
      if (typeof a.note === "string") o.note = a.note.slice(0, 400);
    }
    if (CHOOSING.some(([c]) => c === (input.approval || {}).choose)) out.approval.choose = input.approval.choose;
    return out;
  }

  let state = defaults();
  try {
    state = sanitize(JSON.parse(localStorage.getItem(KEY) || "null"));
  } catch {
    state = defaults();
  }
  const save = () => {
    try {
      localStorage.setItem(KEY, JSON.stringify(state));
    } catch {
      // A private window: the choices last for the page.
    }
  };
  const status = (text) => ($("status").textContent = text);

  // The renderer and the song.
  const renderer = createRenderer($("gl"), $("overlay"));
  const audio = new AudioContext();
  let song = null; // { buffer, analysis, timing, title }
  let source = null;
  let startedAt = 0;
  let position = 0;
  let playing = false;
  let held = null; // reduced motion: the frame held

  async function bitmap(blob) {
    try {
      return await createImageBitmap(blob);
    } catch {
      return null;
    }
  }

  async function loadSong(files) {
    stop();
    $("loading").hidden = false;
    $("loading").textContent = "Reading the song…";
    try {
      const buffer = await audio.decodeAudioData(await files.music.arrayBuffer());
      $("loading").textContent = "Analyzing the music…";
      const analysis = await analyze(buffer, (f) => ($("loading").textContent = `Analyzing the music… ${Math.round(f * 100)}%`));
      const text = files.simfile ? await files.simfile.text() : null;
      const timing = Timing.parse(text);
      renderer.setImage("background", files.background ? await bitmap(files.background) : null);
      renderer.setImage("banner", files.banner ? await bitmap(files.banner) : null);
      let peak = 1e-3;
      const samples = buffer.getChannelData(0);
      for (let k = 0; k < samples.length; k += 7) peak = Math.max(peak, Math.abs(samples[k]));
      song = { buffer, analysis, timing, title: files.title, peak, typical: typical(analysis) };
      position = 0;
      $("seek").max = String(buffer.duration);
      status(`${files.title}: ${buffer.duration.toFixed(1)} s, ${analysis.peaks.length} onsets${text ? "" : ", no simfile (120 BPM assumed)"}.`);
    } catch (e) {
      status(`The song could not be read: ${e.message || e}`);
    } finally {
      $("loading").hidden = true;
    }
  }

  async function loadBundled(id) {
    const s = BUNDLED.find((b) => b.id === id) || BUNDLED[0];
    const get = async (name) => (name ? (await fetch(SONGS + s.id + "/" + encodeURIComponent(name))).blob() : null);
    try {
      const [music, simfile, background, banner] = await Promise.all([get(s.music), get(s.simfile), get(s.background), get(s.banner)]);
      await loadSong({ music, simfile, background, banner, title: s.title });
    } catch (e) {
      status(`Serve the repository root to load the bundled songs (${e.message || e}); or open files.`);
    }
  }

  async function loadFiles(list) {
    const files = [...list];
    const ext = (f) => f.name.toLowerCase().split(".").pop();
    const music = files.find((f) => ["ogg", "opus", "mp3", "wav", "flac", "m4a", "oga"].includes(ext(f)));
    if (!music) return status("No music file among those picked.");
    const simfile = files.find((f) => ext(f) === "ssc") || files.find((f) => ext(f) === "sm");
    const images = files.filter((f) => ["png", "jpg", "jpeg", "gif", "bmp", "webp"].includes(ext(f)));
    const banner = images.find((f) => /(^|[^a-z])(bn|banner)/i.test(f.name));
    const background = images.find((f) => f !== banner);
    await loadSong({ music, simfile, background, banner, title: music.name });
  }

  // Playback.
  function now() {
    return playing ? audio.currentTime - startedAt : position;
  }
  function play() {
    if (!song || playing) return;
    audio.resume();
    source = audio.createBufferSource();
    source.buffer = song.buffer;
    source.connect(audio.destination);
    if (position >= song.buffer.duration) position = 0;
    source.start(0, position);
    startedAt = audio.currentTime - position;
    playing = true;
    source.onended = () => {
      if (!playing) return;
      playing = false;
      position = $("loop").checked ? 0 : song.buffer.duration;
      if ($("loop").checked) play();
      sync();
    };
    sync();
  }
  function stop() {
    if (!playing) return;
    position = now();
    playing = false;
    source.onended = null;
    source.stop();
    sync();
  }
  function seek(t) {
    const was = playing;
    stop();
    position = Math.max(0, t);
    if (was) play();
  }
  function sync() {
    $("play").textContent = playing ? "Pause" : "Play";
    $("play").setAttribute("aria-pressed", String(playing));
  }

  // The frame's music.
  const BASS = [0, 6], MID = [6, 18], TREBLE = [18, 32];
  const clocks = {};
  let lastFrame = performance.now();
  let lastT = 0;

  // The music's signals at the analysis's frame `i`.
  function musicAt(a, i) {
    const bands = a.bandsData.subarray(i * a.bands, (i + 1) * a.bands);
    const mean = ([lo, hi]) => {
      let v = 0;
      for (let k = lo; k < hi; k++) v += bands[k];
      return v / (hi - lo);
    };
    // Onsets held a little, fading (a flash, not a blink).
    let onset = 0;
    for (let k = 0; k < 8 && i - k >= 0; k++) onset = Math.max(onset, a.onset[i - k] * Math.exp(-k * 0.45));
    return {
      loudness: a.loudness[i],
      bass: mean(BASS),
      mid: mean(MID),
      treble: mean(TREBLE),
      onset,
      brightness: a.brightness[i],
      width: a.width[i],
      section: a.section[i],
      bands,
    };
  }

  // Each signal's median and 95th percentile over the song: what stands
  // out of it, for `lift`.
  function typical(a) {
    const series = {};
    for (let i = 0; i < a.frames; i++) {
      const m = musicAt(a, i);
      for (const k of Object.keys(m)) if (k !== "bands") (series[k] ||= []).push(m[k]);
    }
    const at = (sorted, p) => sorted[Math.floor(p * (sorted.length - 1))] || 0;
    const out = {};
    for (const [k, v] of Object.entries(series)) {
      v.sort((x, y) => x - y);
      out[k] = [at(v, 0.5), at(v, 0.95)];
    }
    return out;
  }

  function frameMusic(t, dt) {
    const a = song.analysis;
    const i = Math.min(a.frames - 1, Math.max(0, Math.floor(t / HOP)));
    const { bands, ...music } = musicAt(a, i);
    const beat = Timing.beatAt(song.timing, t);
    const { measure, phase } = Timing.measureAt(song.timing, beat);
    const signals = {
      none: 0,
      ...music,
      beat: beat >= 0 ? Math.exp(-(beat - Math.floor(beat)) * 5) : 0,
      measure: beat >= 0 ? Math.exp(-phase * 4) : 0,
    };
    const data = song.buffer.getChannelData(0);
    const rate = song.buffer.sampleRate;
    return {
      t,
      dt,
      beat,
      measure,
      measurePhase: phase,
      bands,
      signal: (name) => signals[name] ?? 0,
      // A signal above its median over the song, its 95th percentile 1:
      // what stands out of the song (the pulses keep their own range).
      lift: (name) => {
        const v = signals[name] ?? 0, r = song.typical[name];
        return r ? Math.min(1, Math.max(0, (v - r[0]) / Math.max(1e-3, r[1] - r[0]))) : v;
      },
      // A clock running at `speed`, faster with `driver` (never jumping
      // when they change).
      clock: (key, speed, driver) => {
        const k = driver && driver !== "none" ? 0.3 + 1.4 * (signals[driver] ?? 0) : 1;
        clocks[key] = (clocks[key] || 0) + dt * speed * k;
        return clocks[key];
      },
      // The waveform at `u` (0..1) across a 40 ms window, each line a
      // little later.
      wave: (u, line) => {
        const at = Math.floor((t + (u - 0.5) * 0.04 + line * 0.004) * rate);
        return at >= 0 && at < data.length ? data[at] : 0;
      },
      signals,
      index: i,
    };
  }

  // Layout.
  const stage = $("stage");
  let size = [0, 0];
  function resize() {
    const r = stage.getBoundingClientRect();
    const ratio = (window.devicePixelRatio || 1) * state.view.resolution;
    size = [r.width, r.height];
    renderer.resize(r.width, r.height, ratio);
    const tr = $("traces");
    tr.width = Math.round(tr.clientWidth * (window.devicePixelRatio || 1));
    tr.height = Math.round(tr.clientHeight * (window.devicePixelRatio || 1));
  }
  new ResizeObserver(resize).observe(stage);

  function rects() {
    const [w, h] = size;
    if (state.view.mode === "single") return [[state.view.candidate, [0, 0, w, h]]];
    const hw = w / 2, hh = h / 2;
    return CANDIDATES.map((c, k) => [c.id, [(k % 2) * hw, Math.floor(k / 2) * hh, hw, hh]]);
  }

  function draw() {
    const real = performance.now();
    let dt = Math.min(0.1, (real - lastFrame) / 1000);
    lastFrame = real;
    if (!song) return requestAnimationFrame(draw);
    let t = now();
    if (state.view.reduced) {
      held ??= t;
      t = held;
      dt = 0;
    } else {
      held = null;
      // The music's time when it plays: paused, still.
      dt = playing ? Math.max(0, Math.min(0.1, t - lastT)) : 0;
    }
    lastT = t;
    const f = frameMusic(t, dt);
    const ratio = (window.devicePixelRatio || 1) * state.view.resolution;
    renderer.clear();
    for (const [id, rect] of rects()) {
      renderer.draw(id, rect, ratio, f, state.params, state.view);
      if (state.view.mode === "grid") label(id, rect);
    }
    drawTraces(f);
    $("seek").value = String(t);
    $("time").textContent = `${Math.floor(t / 60)}:${(t % 60).toFixed(1).padStart(4, "0")}`;
    $("beat").textContent = f.beat >= 0 ? `beat ${f.beat.toFixed(1)} · m${f.measure + 1}` : "before beat 0";
    requestAnimationFrame(draw);
  }

  function label(id, rect) {
    const ctx = $("overlay").getContext("2d");
    const c = CANDIDATES.find((x) => x.id === id);
    ctx.save();
    ctx.font = "12px system-ui";
    ctx.fillStyle = "rgba(5,2,13,0.7)";
    ctx.fillRect(rect[0] + 8, rect[1] + 8, ctx.measureText(c.label).width + 16, 22);
    ctx.fillStyle = "#f2eee9";
    ctx.fillText(c.label, rect[0] + 16, rect[1] + 23);
    ctx.restore();
  }

  // The traces: each signal over the last 8 s and the next 2.
  function drawTraces(f) {
    const c = $("traces");
    const ctx = c.getContext("2d");
    const ratio = window.devicePixelRatio || 1;
    const w = c.width / ratio, h = c.height / ratio;
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const a = song.analysis;
    const left = 92, span = 10, past = 8;
    const from = f.t - past;
    const x = (t) => left + ((t - from) / span) * (w - left - 8);
    const rows = [
      ["Waveform, now", "wave"],
      ["Loudness", a.loudness],
      ["Bands", "bands"],
      ["Onsets", a.onset],
      ["Brightness", a.brightness],
      ["Stereo width", a.width],
      ["Balance", a.balance],
      ["Sections", a.section],
    ];
    const rh = (h - 18) / rows.length;
    ctx.font = "11px system-ui";
    // Beats and measures.
    const b0 = Math.ceil(Timing.beatAt(song.timing, from));
    for (let b = Math.max(0, b0); ; b++) {
      const t = beatTime(b);
      if (t > from + span) break;
      const { phase } = Timing.measureAt(song.timing, b);
      ctx.fillStyle = phase < 1e-6 ? "rgba(143,134,255,0.45)" : "rgba(143,134,255,0.15)";
      ctx.fillRect(x(t), 0, 1, h - 18);
      if (b > 2000) break;
    }
    rows.forEach(([name, values], r) => {
      const y = r * rh;
      ctx.fillStyle = "#9a93a8";
      ctx.fillText(name, 8, y + rh / 2 + 4);
      if (values === "wave") {
        // The waveform itself, live: 40 ms around now, to the song's peak.
        ctx.strokeStyle = "#cfc8dc";
        ctx.beginPath();
        for (let s = 0; s <= 200; s++) {
          const v = f.wave(s / 200, 0) / song.peak;
          const px = left + (s / 200) * (w - left - 8), py = y + rh / 2 - v * rh * 0.45;
          if (s) ctx.lineTo(px, py);
          else ctx.moveTo(px, py);
        }
        ctx.stroke();
        return;
      }
      if (values === "bands") {
        const step = Math.max(1, Math.floor(span / HOP / (w - left)));
        for (let k = Math.max(0, Math.floor(from / HOP)); k < Math.min(a.frames, (from + span) / HOP); k += step) {
          for (let b = 0; b < a.bands; b++) {
            const v = a.bandsData[k * a.bands + b];
            if (v < 0.15) continue;
            ctx.fillStyle = `rgba(207,200,220,${v * v})`;
            ctx.fillRect(x(k * HOP), y + rh - ((b + 1) / a.bands) * rh, Math.max(1, (step * HOP * (w - left)) / span), rh / a.bands + 0.5);
          }
        }
        return;
      }
      ctx.strokeStyle = name === "Onsets" ? "#ffb48a" : "#8f86ff";
      ctx.beginPath();
      let first = true;
      for (let k = Math.max(0, Math.floor(from / HOP)); k < Math.min(a.frames, (from + span) / HOP); k++) {
        const v = name === "Balance" ? 0.5 + values[k] / 2 : values[k];
        const px = x(k * HOP), py = y + rh - 3 - v * (rh - 6);
        if (first) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
        first = false;
      }
      ctx.stroke();
      if (name === "Onsets") {
        ctx.fillStyle = "#ffb48a";
        for (const p of a.peaks) {
          const t = p * HOP;
          if (t >= from && t <= from + span) ctx.fillRect(x(t) - 1, y + 2, 2, 5);
        }
      }
    });
    // Now.
    ctx.fillStyle = "#f2eee9";
    ctx.fillRect(x(f.t), 0, 1.5, h - 18);
    ctx.fillStyle = "#9a93a8";
    ctx.fillText(`${past} s ago`, left, h - 4);
    ctx.fillText("now", x(f.t) - 10, h - 4);
  }

  // The song second of `beat` (a beat's tick on the traces).
  function beatTime(beat) {
    let lo = -60, hi = song.buffer.duration + 60;
    for (let k = 0; k < 40; k++) {
      const mid = (lo + hi) / 2;
      if (Timing.beatAt(song.timing, mid) < beat) lo = mid;
      else hi = mid;
    }
    return hi;
  }

  // The controls.
  function control(p, value, set) {
    const label = document.createElement("label");
    const name = document.createElement("span");
    name.textContent = p.label;
    label.append(name);
    if (p.kind === "range") {
      const out = document.createElement("output");
      const input = Object.assign(document.createElement("input"), { type: "range", min: p.min, max: p.max, step: p.step, value });
      const show = () => (out.textContent = Number(input.value).toFixed(p.step < 0.1 ? 2 : p.step < 1 ? 1 : 0));
      show();
      input.addEventListener("input", () => {
        show();
        set(Number(input.value));
      });
      label.append(out, input);
    } else if (p.kind === "select") {
      const sel = document.createElement("select");
      for (const [v, text] of p.options) sel.append(new Option(text, v, false, v === value));
      sel.addEventListener("change", () => set(sel.value));
      label.append(sel);
    } else {
      const box = Object.assign(document.createElement("input"), { type: "checkbox", checked: value });
      box.addEventListener("change", () => set(box.checked));
      label.prepend(box);
      label.classList.add("check");
    }
    return label;
  }

  function buildPanels() {
    const panels = $("panels");
    panels.textContent = "";
    for (const c of CANDIDATES) {
      const shown = state.view.mode === "grid" || state.view.candidate === c.id || c.id === baseOf();
      if (!shown) continue;
      const d = document.createElement("details");
      d.open = state.view.mode === "single" ? c.id === state.view.candidate : false;
      const s = document.createElement("summary");
      s.textContent = c.label + (c.id !== state.view.candidate && state.view.mode === "single" ? " (as the base)" : "");
      const g = document.createElement("div");
      g.className = "control-group";
      for (const p of c.params) {
        g.append(control(p, state.params[c.id][p.id], (v) => {
          state.params[c.id][p.id] = v;
          save();
          if (p.id === "base") buildPanels();
        }));
      }
      d.append(s, g);
      panels.append(d);
    }
  }

  // The base the shown candidate draws over, when another candidate.
  function baseOf() {
    const p = state.params[state.view.candidate];
    return p && p.base && p.base !== "ground" ? p.base : null;
  }

  function buildApproval() {
    const g = $("approval");
    g.textContent = "";
    const how = { label: "A song gets", kind: "select", options: CHOOSING };
    g.append(control(how, state.approval.choose, (v) => {
      state.approval.choose = v;
      save();
    }));
    for (const c of CANDIDATES) {
      const a = state.approval.candidates[c.id];
      const box = document.createElement("div");
      box.className = "candidate";
      const title = document.createElement("strong");
      title.textContent = c.label;
      const row = document.createElement("div");
      row.className = "row";
      row.append(
        control({ label: "Keep", kind: "check" }, a.keep, (v) => { a.keep = v; save(); }),
        control({ label: "Works over a picture", kind: "check" }, a.overPicture, (v) => { a.overPicture = v; save(); }),
      );
      const note = Object.assign(document.createElement("input"), { type: "text", placeholder: "A note", value: a.note });
      note.addEventListener("change", () => {
        a.note = note.value.slice(0, 400);
        save();
      });
      box.append(title, row, note);
      g.append(box);
    }
  }

  function buildView() {
    const sel = $("song");
    sel.textContent = "";
    for (const s of BUNDLED) sel.append(new Option(s.title, s.id, false, s.id === state.song));
    const cand = $("candidate");
    cand.textContent = "";
    for (const c of CANDIDATES) cand.append(new Option(c.label, c.id, false, c.id === state.view.candidate));
    const pal = $("palette");
    pal.textContent = "";
    for (const [k, p] of Object.entries(PALETTES)) pal.append(new Option(p.label, k, false, k === state.view.palette));
    $("mode").value = state.view.mode;
    $("candidate-label").hidden = state.view.mode === "grid";
    $("over").checked = state.view.overPicture;
    $("field").checked = state.view.showField;
    $("filter").value = String(state.view.fieldFilter);
    $("brightness").value = String(state.view.pictureBrightness);
    $("reduced").checked = state.view.reduced;
    $("resolution").value = String(state.view.resolution);
  }

  function rebuild() {
    buildView();
    buildPanels();
    buildApproval();
    resize();
  }

  // Wiring.
  $("song").addEventListener("change", () => {
    state.song = $("song").value;
    save();
    loadBundled(state.song);
  });
  $("files").addEventListener("change", (e) => e.target.files.length && loadFiles(e.target.files));
  $("mode").addEventListener("change", () => {
    state.view.mode = $("mode").value;
    save();
    rebuild();
  });
  $("candidate").addEventListener("change", () => {
    state.view.candidate = $("candidate").value;
    save();
    buildPanels();
  });
  $("palette").addEventListener("change", () => {
    state.view.palette = $("palette").value;
    save();
  });
  for (const [id, key] of [["over", "overPicture"], ["field", "showField"], ["reduced", "reduced"]]) {
    $(id).addEventListener("change", () => {
      state.view[key] = $(id).checked;
      save();
    });
  }
  $("filter").addEventListener("input", () => {
    state.view.fieldFilter = Number($("filter").value);
    save();
  });
  $("brightness").addEventListener("input", () => {
    state.view.pictureBrightness = Number($("brightness").value);
    save();
  });
  $("resolution").addEventListener("change", () => {
    state.view.resolution = Number($("resolution").value);
    save();
    resize();
  });
  $("play").addEventListener("click", () => (playing ? stop() : play()));
  $("seek").addEventListener("input", () => seek(Number($("seek").value)));
  document.addEventListener("keydown", (e) => {
    if (e.target.matches("input, select") || e.code !== "Space") return;
    e.preventDefault();
    playing ? stop() : play();
  });
  $("export").addEventListener("click", () => {
    const out = {
      version: VERSION,
      study: "tools/background-design",
      exported: new Date().toISOString().slice(0, 10),
      approval: state.approval,
      view: { palette: state.view.palette, fieldFilter: state.view.fieldFilter, pictureBrightness: state.view.pictureBrightness },
      params: state.params,
    };
    const url = URL.createObjectURL(new Blob([JSON.stringify(out, null, 2) + "\n"], { type: "application/json" }));
    Object.assign(document.createElement("a"), { href: url, download: "approved.json" }).click();
    URL.revokeObjectURL(url);
    status("Exported approved.json.");
  });
  $("import").addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    try {
      const input = JSON.parse(await file.text());
      state = sanitize({ ...state, ...input, view: { ...state.view, ...(input.view || {}) } });
      save();
      rebuild();
      status(`Imported ${file.name}.`);
    } catch (err) {
      status(`Not a choices file: ${err.message || err}`);
    }
    e.target.value = "";
  });
  $("reset").addEventListener("click", () => {
    state = defaults();
    save();
    rebuild();
    status("Back to the study's defaults.");
  });

  rebuild();
  loadBundled(state.song);
  requestAnimationFrame(draw);

  // For checks from a headless browser.
  window.__BG_STUDY = {
    state: () => state,
    song: () => song && { title: song.title, duration: song.buffer.duration, frames: song.analysis.frames, peaks: song.analysis.peaks.length },
    play,
    stop,
    seek,
    playing: () => playing,
    now,
    // The music's signals at second `t`, for checks.
    signals: (t) => {
      if (!song) return null;
      const f = frameMusic(t, 0);
      return { ...f.signals, lifted: Object.fromEntries(Object.keys(f.signals).map((k) => [k, f.lift(k)])) };
    },
  };
})();
