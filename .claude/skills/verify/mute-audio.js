// Silences Web Audio output for browsers without a mute switch (Playwright's
// WebKit; Chrome has --mute-audio, firefox.mjs sets media.volume_scale).
// Install with `page.addInitScript({ path: '<this file>' })`.
//
// Anything connected to a context's destination goes through a zero gain
// node instead, so the graph still renders and the game's output meter
// (master → analyser → destination) still reads the real level in
// __DDI_DEBUG.audio.
(() => {
  const connect = AudioNode.prototype.connect;
  const silent = new WeakMap();
  AudioNode.prototype.connect = function (target, ...rest) {
    if (!(target instanceof AudioDestinationNode)) return connect.call(this, target, ...rest);
    let gain = silent.get(target);
    if (!gain) {
      gain = target.context.createGain();
      gain.gain.value = 0;
      connect.call(gain, target);
      silent.set(target, gain);
    }
    connect.call(this, gain, ...rest);
    return target;
  };
})();
