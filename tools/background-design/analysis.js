// The music's analysis for the background study, made once when a song
// loads, from the samples Web Audio decoded: what each visualization may
// follow, a value every HOP seconds. A design approximation of what the
// game would compute at load (docs/plans/phase-8-gaps.md, step 5), not
// its reference.
"use strict";

(() => {
  const HOP = 0.02;
  const SIZE = 2048;
  const BANDS = 32;
  const LOW_HZ = 30;
  const HIGH_HZ = 16000;

  // An in-place radix-2 FFT of `re` and `im` (length a power of two).
  function fft(re, im) {
    const n = re.length;
    for (let i = 1, j = 0; i < n; i++) {
      let bit = n >> 1;
      for (; j & bit; bit >>= 1) j ^= bit;
      j ^= bit;
      if (i < j) {
        [re[i], re[j]] = [re[j], re[i]];
        [im[i], im[j]] = [im[j], im[i]];
      }
    }
    for (let len = 2; len <= n; len <<= 1) {
      const angle = (-2 * Math.PI) / len;
      const wr = Math.cos(angle), wi = Math.sin(angle);
      for (let i = 0; i < n; i += len) {
        let cr = 1, ci = 0;
        for (let k = 0; k < len / 2; k++) {
          const a = i + k, b = a + len / 2;
          const tr = re[b] * cr - im[b] * ci;
          const ti = re[b] * ci + im[b] * cr;
          re[b] = re[a] - tr;
          im[b] = im[a] - ti;
          re[a] += tr;
          im[a] += ti;
          const nr = cr * wr - ci * wi;
          ci = cr * wi + ci * wr;
          cr = nr;
        }
      }
    }
  }

  // A value at percentile `p` (0..1) of `values`.
  function percentile(values, p) {
    const sorted = Float32Array.from(values).sort();
    return sorted[Math.min(sorted.length - 1, Math.max(0, Math.floor(p * (sorted.length - 1))))] || 0;
  }

  // `values` averaged over `seconds` around each hop.
  function smooth(values, seconds) {
    const half = Math.max(1, Math.round(seconds / HOP / 2));
    const out = new Float32Array(values.length);
    let sum = 0, count = 0;
    for (let i = 0; i < Math.min(half, values.length); i++) { sum += values[i]; count++; }
    for (let i = 0; i < values.length; i++) {
      const add = i + half, drop = i - half - 1;
      if (add < values.length) { sum += values[add]; count++; }
      if (drop >= 0) { sum -= values[drop]; count--; }
      out[i] = sum / count;
    }
    return out;
  }

  // `values` mapped so that percentiles `lo` and `hi` become 0 and 1.
  function stretch(values, lo, hi) {
    const a = percentile(values, lo), b = percentile(values, hi);
    const span = Math.max(b - a, 1e-6);
    return values.map((v) => Math.min(1, Math.max(0, (v - a) / span)));
  }

  /**
   * Analyzes `buffer` (an AudioBuffer), giving `progress(fraction)` a turn
   * as it goes. Every array has one value a hop:
   * - loudness: RMS level, -60..0 dBFS as 0..1;
   * - bands: BANDS log-spaced bands from LOW_HZ to HIGH_HZ, each 0..1 over
   *   60 dB under the loudest band of the song (row-major, BANDS a hop);
   * - onset: spectral flux of the log spectrum, normalized (0..1), and
   *   `peaks`, the hops where a sound starts;
   * - brightness: the spectral centroid, 200 Hz..8 kHz on a log scale;
   * - width: side over mid level (0 mono .. 1 wide); balance: -1 left ..
   *   1 right;
   * - section: an 8-second loudness, stretched over the song's 10th to
   *   90th percentiles (quiet stretches, builds, drops).
   */
  async function analyze(buffer, progress = () => {}) {
    const rate = buffer.sampleRate;
    const left = buffer.getChannelData(0);
    const right = buffer.numberOfChannels > 1 ? buffer.getChannelData(1) : left;
    const hop = Math.round(HOP * rate);
    const frames = Math.max(1, Math.floor(buffer.length / hop));
    const window = new Float32Array(SIZE);
    for (let i = 0; i < SIZE; i++) window[i] = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / (SIZE - 1));
    // Each band's bins.
    const edges = [];
    for (let b = 0; b <= BANDS; b++) {
      const hz = LOW_HZ * Math.pow(HIGH_HZ / LOW_HZ, b / BANDS);
      edges.push(Math.min(SIZE / 2 - 1, Math.max(1, Math.round((hz * SIZE) / rate))));
    }
    const loudness = new Float32Array(frames);
    const bandsDb = new Float32Array(frames * BANDS);
    const flux = new Float32Array(frames);
    const brightness = new Float32Array(frames);
    const width = new Float32Array(frames);
    const balance = new Float32Array(frames);
    const re = new Float32Array(SIZE), im = new Float32Array(SIZE);
    let previous = new Float32Array(SIZE / 2);
    let current = new Float32Array(SIZE / 2);
    let yielded = performance.now();
    for (let f = 0; f < frames; f++) {
      const center = f * hop + (hop >> 1);
      const start = center - SIZE / 2;
      let sum = 0, mid = 0, side = 0, l2 = 0, r2 = 0;
      for (let i = 0; i < SIZE; i++) {
        const at = start + i;
        const l = at >= 0 && at < left.length ? left[at] : 0;
        const r = at >= 0 && at < right.length ? right[at] : 0;
        const m = (l + r) * 0.5, s = (l - r) * 0.5;
        re[i] = m * window[i];
        im[i] = 0;
        if (i >= SIZE / 2 - hop / 2 && i < SIZE / 2 + hop / 2) {
          sum += m * m;
          mid += m * m;
          side += s * s;
          l2 += l * l;
          r2 += r * r;
        }
      }
      loudness[f] = Math.min(1, Math.max(0, (10 * Math.log10(sum / hop + 1e-12) + 60) / 60));
      width[f] = Math.min(1, Math.sqrt(side / (mid + 1e-12)));
      balance[f] = (r2 - l2) / (r2 + l2 + 1e-12);
      fft(re, im);
      let weighted = 0, total = 0, rise = 0;
      for (let k = 1; k < SIZE / 2; k++) {
        const power = re[k] * re[k] + im[k] * im[k];
        const mag = Math.log1p(Math.sqrt(power) * 10);
        current[k] = mag;
        rise += Math.max(0, mag - previous[k]);
        const hz = (k * rate) / SIZE;
        weighted += hz * power;
        total += power;
      }
      flux[f] = rise;
      const centroid = total > 0 ? weighted / total : 0;
      brightness[f] = Math.min(1, Math.max(0, Math.log(Math.max(centroid, 200) / 200) / Math.log(8000 / 200)));
      for (let b = 0; b < BANDS; b++) {
        let p = 0;
        const lo = edges[b], hi = Math.max(edges[b + 1], lo + 1);
        for (let k = lo; k < hi; k++) p += Math.expm1(current[k]) ** 2 / 100;
        bandsDb[f * BANDS + b] = 10 * Math.log10(p / (hi - lo) + 1e-12);
      }
      [previous, current] = [current, previous];
      if (performance.now() - yielded > 30) {
        progress(f / frames);
        await new Promise((r) => setTimeout(r, 0));
        yielded = performance.now();
      }
    }
    let top = -Infinity;
    for (const v of bandsDb) if (v > top) top = v;
    const bands = bandsDb.map((v) => Math.min(1, Math.max(0, (v - (top - 60)) / 60)));
    // Onsets: the flux over its local average, then stretched.
    const base = smooth(flux, 0.5);
    const lifted = flux.map((v, i) => Math.max(0, v - base[i]));
    const onset = stretch(lifted, 0.5, 0.995);
    const peaks = [];
    for (let i = 1; i < frames - 1; i++) {
      if (onset[i] > 0.35 && onset[i] >= onset[i - 1] && onset[i] > onset[i + 1]) {
        if (!peaks.length || i - peaks[peaks.length - 1] > 3) peaks.push(i);
      }
    }
    const section = stretch(smooth(loudness, 8), 0.1, 0.9);
    progress(1);
    return { hop: HOP, frames, bands: BANDS, loudness, bandsData: bands, onset, peaks, brightness, width, balance, section };
  }

  window.BgAnalysis = { analyze, HOP, BANDS, LOW_HZ, HIGH_HZ };
})();
