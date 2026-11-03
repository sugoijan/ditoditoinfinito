// A simfile's timing for the background study: its offset, BPM changes,
// stops and time signatures (a .sm's, or a .ssc's song-level tags), and
// the beat and measure at a song second, as StepMania times them
// (`time(beat 0) = -offset`). Enough for a study: warps, delays and a
// chart's own timing are left out.
"use strict";

(() => {
  // The song-level value of tag `name` (before a .ssc's first #NOTEDATA).
  function tag(text, name) {
    const head = text.split(/#NOTEDATA\s*:/i)[0];
    const m = head.match(new RegExp(`#${name}\\s*:([^;]*);`, "i"));
    return m ? m[1].trim() : null;
  }

  // `beat=value[=value]` pairs.
  function pairs(value) {
    if (!value) return [];
    return value
      .split(",")
      .map((p) => p.split("=").map((v) => parseFloat(v.trim())))
      .filter((p) => p.every(Number.isFinite));
  }

  /** The timing of a simfile's `text`; a constant one of `bpm` for none. */
  function parse(text, bpm = 120) {
    if (!text) return constant(bpm, 0);
    const offset = parseFloat(tag(text, "OFFSET") ?? "0") || 0;
    let bpms = pairs(tag(text, "BPMS")).filter(([, v]) => v > 0);
    if (!bpms.length) bpms = [[0, bpm]];
    bpms.sort((a, b) => a[0] - b[0]);
    bpms[0][0] = 0;
    const stops = pairs(tag(text, "STOPS")).filter(([b, s]) => b >= 0 && s > 0);
    stops.sort((a, b) => a[0] - b[0]);
    let signatures = pairs(tag(text, "TIMESIGNATURES")).filter(([, n, d]) => n > 0 && d > 0);
    if (!signatures.length) signatures = [[0, 4, 4]];
    return build(offset, bpms, stops, signatures);
  }

  /** A constant `bpm`, beat 0 at `offsetSeconds` into the music. */
  function constant(bpm, offsetSeconds) {
    return build(-offsetSeconds, [[0, bpm]], [], [[0, 4, 4]]);
  }

  // The timing with each event's second worked out, for both ways.
  function build(offset, bpms, stops, signatures) {
    // Events in beat order: a BPM change, then a stop on the same beat.
    const events = [
      ...bpms.map(([beat, bpm]) => ({ beat, bpm })),
      ...stops.map(([beat, seconds]) => ({ beat, stop: seconds })),
    ].sort((a, b) => a.beat - b.beat || (a.bpm ? -1 : 1));
    let seconds = -offset, beat = 0, bpm = bpms[0][1];
    const marks = [];
    for (const e of events) {
      seconds += ((e.beat - beat) * 60) / bpm;
      beat = e.beat;
      if (e.bpm) {
        bpm = e.bpm;
        marks.push({ beat, seconds, bpm, stop: 0 });
      } else {
        marks.push({ beat, seconds, bpm, stop: e.stop });
        seconds += e.stop;
      }
    }
    return { offset, bpms, stops, signatures, marks };
  }

  /** The beat at song second `t`. */
  function beatAt(timing, t) {
    let beat = 0, seconds = -timing.offset, bpm = timing.bpms[0][1];
    for (const m of timing.marks) {
      if (m.seconds > t) break;
      beat = m.beat;
      seconds = m.seconds;
      bpm = m.bpm;
      if (m.stop) {
        if (t < m.seconds + m.stop) return beat;
        seconds += m.stop;
      }
    }
    return beat + ((t - seconds) * bpm) / 60;
  }

  /** The measure at `beat` and how far into it (0..1), by the signatures. */
  function measureAt(timing, beat) {
    let measure = 0, from = 0, length = 4;
    for (const [at, n, d] of timing.signatures) {
      if (at > beat) break;
      measure += (at - from) / length;
      from = at;
      length = (4 * n) / d;
    }
    const m = measure + (beat - from) / length;
    return { measure: Math.floor(m), phase: m - Math.floor(m) };
  }

  window.BgTiming = { parse, constant, beatAt, measureAt };
})();
