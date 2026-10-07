// Fake Gamepad API for headless tests. Install with
// `page.addInitScript({ path: '<this file>' })`, then drive it with
// `window.__DDI_FAKE_PAD` from `page.evaluate`.
//
//   __DDI_FAKE_PAD.add({ id, mapping: '' | 'standard', buttons: 17, axes: 4,
//                        hatAxis: 9, trustTimestamp: true }) -> index
//   __DDI_FAKE_PAD.press(index, 'button:3' | 'axis:1-' | 'hat:9:left')
//   __DDI_FAKE_PAD.release(index, control)
//   __DDI_FAKE_PAD.schedule(hostMs, control, pressed, index = 0)
//   __DDI_FAKE_PAD.disconnect(index)
//
// `schedule` (used by the app's `auto=pad` mode) applies the edge when
// `getGamepads()` is called at or after `hostMs` on the performance timeline.
// With `trustTimestamp` the pad's `timestamp` is the exact edge time (a
// browser that stamps each report); without it `timestamp` stays 0 and the
// app falls back to its poll time.
(() => {
  const HAT_NEUTRAL = 9 / 7; // 1.2857…: the HID null position scaled like the rest
  const HAT_DIRS = ['up', 'right', 'down', 'left'];
  const pads = [];

  function add(opts = {}) {
    const nb = opts.buttons ?? 17;
    const na = opts.axes ?? 4;
    const pad = {
      id: opts.id ?? 'Fake Pad (Vendor: 1234 Product: 5678)',
      index: pads.length,
      mapping: opts.mapping ?? 'standard',
      connected: true,
      timestamp: 0,
      buttons: Array.from({ length: nb }, () => ({ pressed: false, touched: false, value: 0 })),
      axes: Array.from({ length: na }, (_, i) => (opts.hatAxis === i ? HAT_NEUTRAL : 0)),
      _trust: opts.trustTimestamp ?? true,
      _schedule: [],
      _hats: {},
    };
    pads.push(pad);
    return pad.index;
  }

  function hatValue(dirs) {
    const [u, r, d, l] = HAT_DIRS.map((k) => dirs.has(k));
    let p = null;
    if (u && r) p = 1;
    else if (d && r) p = 3;
    else if (d && l) p = 5;
    else if (u && l) p = 7;
    else if (u) p = 0;
    else if (r) p = 2;
    else if (d) p = 4;
    else if (l) p = 6;
    return p === null ? HAT_NEUTRAL : (p * 2) / 7 - 1;
  }

  function apply(pad, control, pressed) {
    let m;
    if ((m = /^button:(\d+)$/.exec(control))) {
      const b = pad.buttons[+m[1]];
      b.pressed = pressed;
      b.touched = pressed;
      b.value = pressed ? 1 : 0;
    } else if ((m = /^axis:(\d+)([+-])$/.exec(control))) {
      pad.axes[+m[1]] = pressed ? (m[2] === '+' ? 1 : -1) : 0;
    } else if ((m = /^hat:(\d+):(up|right|down|left)$/.exec(control))) {
      const i = +m[1];
      const dirs = (pad._hats[i] ??= new Set());
      if (pressed) dirs.add(m[2]);
      else dirs.delete(m[2]);
      pad.axes[i] = hatValue(dirs);
    } else {
      throw new Error('fake pad: unknown control ' + control);
    }
  }

  function advance(now) {
    for (const pad of pads) {
      if (!pad._schedule.length) continue;
      pad._schedule.sort((a, b) => a.t - b.t || a.seq - b.seq);
      while (pad._schedule.length && pad._schedule[0].t <= now) {
        const e = pad._schedule.shift();
        apply(pad, e.control, e.pressed);
        if (pad._trust) pad.timestamp = Math.max(pad.timestamp, e.t);
      }
    }
  }

  let seq = 0;
  navigator.getGamepads = () => {
    advance(performance.now());
    return pads.map((p) => (p.connected ? p : null));
  };

  window.__DDI_FAKE_PAD = {
    pads,
    add,
    press(index, control) {
      const pad = pads[index];
      apply(pad, control, true);
      if (pad._trust) pad.timestamp = performance.now();
    },
    release(index, control) {
      const pad = pads[index];
      apply(pad, control, false);
      if (pad._trust) pad.timestamp = performance.now();
    },
    schedule(hostMs, control, pressed, index = 0) {
      pads[index]._schedule.push({ t: hostMs, control, pressed, seq: seq++ });
    },
    disconnect(index) {
      pads[index].connected = false;
    },
  };
})();
