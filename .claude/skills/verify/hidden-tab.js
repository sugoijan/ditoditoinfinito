// EMULATION of a hidden tab for headless browsers (which never hide a page):
// what a browser does to a page in the background, as far as the page's own
// code can tell: no animation frames, timers at most once every `floor` ms
// (Chrome and Firefox: 1000; Chrome after five minutes: 60000),
// `document.hidden`, and the `visibilitychange` event. The network is left
// alone, as browsers leave it. window.__hide(floorMs), window.__show().
(() => {
  const w = window;
  const real = {
    raf: w.requestAnimationFrame.bind(w), caf: w.cancelAnimationFrame.bind(w),
    st: w.setTimeout.bind(w), si: w.setInterval.bind(w),
    ct: w.clearTimeout.bind(w), ci: w.clearInterval.bind(w),
  };
  let hidden = false;
  let floor = 1000;
  const held = new Map();
  let fake = 1e9;
  w.requestAnimationFrame = (cb) => {
    if (!hidden) return real.raf(cb);
    const id = fake++;
    held.set(id, cb);
    return id;
  };
  w.cancelAnimationFrame = (id) => { if (!held.delete(id)) real.caf(id); };
  // A timer that comes due while hidden runs at the next wake-up (a multiple
  // of `floor` on the wall clock); an interval at most once a wake-up. A
  // timer cleared meanwhile never runs (as a browser's: the page may drop
  // what the timer would call).
  const deferred = new Map();
  const wake = () => { const now = Date.now(); return Math.max(1, Math.ceil((now + 1) / floor) * floor - now); };
  w.setTimeout = (cb, ms, ...a) => {
    const id = real.st(() => {
      if (typeof cb !== 'function') return;
      if (hidden) {
        deferred.set(id, real.st(() => { deferred.delete(id); cb(...a); }, wake()));
      } else cb(...a);
    }, ms);
    return id;
  };
  w.setInterval = (cb, ms, ...a) => {
    const id = real.si(() => {
      if (!hidden) return cb(...a);
      if (deferred.has(id)) return;
      deferred.set(id, real.st(() => { deferred.delete(id); cb(...a); }, wake()));
    }, ms);
    return id;
  };
  const clear = (id) => { if (deferred.has(id)) { real.ct(deferred.get(id)); deferred.delete(id); } };
  w.clearTimeout = (id) => { clear(id); real.ct(id); };
  w.clearInterval = (id) => { clear(id); real.ci(id); };
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => (hidden ? 'hidden' : 'visible') });
  w.__hide = (f = 1000) => { floor = f; hidden = true; document.dispatchEvent(new Event('visibilitychange')); };
  w.__show = () => {
    hidden = false;
    for (const [, cb] of held) real.raf(cb);
    held.clear();
    document.dispatchEvent(new Event('visibilitychange'));
  };
  w.__hiddenNow = () => hidden;
})();
