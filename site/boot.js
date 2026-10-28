// The page's boot script: the loader's state (the art laid out and fed, the
// bar, the blocked and crashed overlays), the browser checks and Trunk's
// initializer hooks. Minified by `xtask minify-boot` in release builds into
// target/boot/, which index.html inlines (AGENTS.md, "Deployment").
(() => {
  const state = {
    progress: 0,
    blocked: null,
    ready: false,
    crashed: false,
  };
  const dom = {
    ready: false,
    shell: null,
    canvas: null,
    blocked: null,
    blockedMessage: null,
    blockedHint: null,
  };
  const clamp01 = (value) => Math.min(1, Math.max(0, value));
  const art = () => window.__DDI_ART || null;
  // The art (boot-art.js) laid out with the title and the bar where
  // the start menu's title and first choice land, so that the
  // shell's fade shows the menu in place: the theme's virtual
  // screen (ddi_actor::screen::VirtualScreen: 1280x720 stretched to
  // the window's aspect, fitted to it), the narrow fit of a portrait
  // window (ddi_ui::screens::narrow_fit) and the title menu's layout
  // (ddi_ui::screens::start); the menu's rain in the same layout.
  const ASPECT = 4.990068;
  const fit = () => {
    if (!dom.canvas || !art()) {
      return;
    }
    const W = window.innerWidth, H = window.innerHeight;
    const native = 1280 / 720, aspect = W / H;
    let vw = 1280, vh = 720;
    if (aspect > native) {
      vw = Math.ceil(1280 * aspect / native);
      vw -= vw % 2;
    } else if (aspect < native) {
      vh = Math.ceil(720 * native / aspect);
    }
    const k = Math.min(W / vw, H / vh);
    const ox = (W - vw * k) / 2, oy = (H - vh * k) / 2;
    const narrow = vw < vh;
    const z = narrow ? vw / 480 : 1, lw = narrow ? 480 : vw, lh = vh / z, s = k * z;
    const row = Math.max(52, 44 / s), menu = 4 * (row + 12) - 12;
    const th = Math.min(104, Math.max(24, lh - menu - 36));
    const top = Math.max(8, (lh - menu - th - 20) / 2);
    const w = Math.min(520, lw - 32, th * ASPECT) * s, h = w / ASPECT;
    const mid = ox + lw * s / 2, bw = Math.min(380, lw - 32) * s;
    art().layout({
      canvas: dom.canvas,
      W,
      H,
      dpr: Math.min(window.devicePixelRatio || 1, 2),
      title: { x: mid - w / 2, y: oy + (top + th / 2) * s - h / 2, w, h },
      bar: { x: mid - bw / 2, y: oy + (top + th + 20 + row / 2) * s, w: bw },
      lw,
      lh,
      s,
    });
  };
  const resolveDom = () => {
    if (dom.ready) {
      return dom;
    }
    const shell = document.getElementById("boot-shell");
    if (!shell) {
      return null;
    }
    dom.shell = shell;
    dom.canvas = document.getElementById("boot-canvas");
    dom.blocked = document.getElementById("boot-blocked");
    dom.blockedMessage = document.getElementById("boot-blocked-message");
    dom.blockedHint = document.getElementById("boot-blocked-hint");
    dom.ready = true;
    // The art runs on the wall clock, as the game's title and rain
    // do (ddi_ui::screens::Live, the start menu's background): they
    // carry over the fade, and a reload does not start them over.
    // Reduced motion holds it at zero, as the game does.
    if (dom.canvas && art()) {
      fit();
      window.addEventListener("resize", fit);
      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
        art().still(true);
      }
    }
    return dom;
  };
  const applyState = () => {
    const root = resolveDom();
    if (!root) {
      return;
    }
    if (art()) {
      art().progress(clamp01(state.progress));
    }
    if (root.shell) {
      if (state.blocked) {
        root.shell.classList.add("is-blocked");
        root.shell.classList.remove("is-loading");
      } else if (!state.ready) {
        root.shell.classList.add("is-loading");
        root.shell.classList.remove("is-blocked");
      }
      if (state.ready) {
        root.shell.classList.add("is-ready");
      }
    }
    if (state.blocked && root.blocked) {
      root.blocked.classList.remove("hidden");
      if (root.blockedMessage) {
        root.blockedMessage.textContent = state.blocked.message;
      }
      if (root.blockedHint) {
        root.blockedHint.textContent = state.blocked.hint || "";
      }
    }
  };
  // The bar never goes back, and an unknown amount keeps it where
  // it is: it starts at the 4% the stylesheet draws it at.
  const setProgress = (value) => {
    if (value == null || value <= state.progress) {
      return;
    }
    state.progress = value;
    applyState();
  };
  // The bar's share for the game's download; the rest is what the
  // game readies before its first screen (the fonts, the device, the
  // screen's textures, its skin and its entrance), reported through
  // setStartProgress as each comes in, so the bar fills as the fade
  // begins. The share is the download's expected time against the
  // rest's, so the bar keeps one pace across both: the link measured
  // as the download goes (counted on at seven tenths: the measure runs
  // high, and a bar short of the mark looks better than one past it),
  // the rest's bytes and own time as the game left them last time
  // (ddi.boot.v1; a guess the first time). Until the link is measured,
  // three fifths; the bar never goes back, so the share is settled
  // over the download's first half, not changed in its second.
  const PLAN_KEY = "ddi.boot.v1";
  const plan = (() => {
    try {
      const [b, c] = String(localStorage.getItem(PLAN_KEY) || "").split(";").map(Number);
      if (b > 0 && c > 0) return [b, c];
    } catch (e) {}
    return [8e6, 1200];
  })();
  let share = 0.6, bandwidth = 0, began = 0, firstLoaded = 0;
  const measure = (loaded, total) => {
    const now = performance.now();
    if (!began) {
      began = now;
      firstLoaded = loaded;
      return;
    }
    const seconds = (now - began) / 1000;
    if (seconds < 0.2 || loaded - firstLoaded < 2e5) return;
    bandwidth = (loaded - firstLoaded) / seconds;
    if (loaded > total / 2) return;
    const counted = bandwidth * 0.7;
    const download = total / counted, rest = plan[0] / counted + plan[1] / 1000;
    share = Math.min(0.9, Math.max(0.2, download / (download + rest)));
  };
  const setDownloadProgress = (ratio, loaded, total) => {
    if (ratio != null && total > 0) measure(loaded, total);
    setProgress(ratio == null ? null : clamp01(ratio) * share);
  };
  const setStartProgress = (ratio) => {
    setProgress(share + clamp01(ratio) * (1 - share));
  };
  const fail = (code, message, hint) => {
    state.blocked = {
      code: code || "blocked",
      message: message || "This browser cannot run the game.",
      hint: hint || "",
    };
    applyState();
    if (art()) {
      art().stop();
    }
  };
  const CRASH_RELOADS_KEY = "ddi.crash.reloads";
  const CRASH_MAX_RELOADS = 2;
  const CRASH_WINDOW_MS = 60000;
  const CRASH_COUNTER_RESET_MS = 30000;
  const readCrashReloads = () => {
    try {
      const raw = window.sessionStorage?.getItem(CRASH_RELOADS_KEY);
      if (!raw) {
        return null;
      }
      const parsed = JSON.parse(raw);
      if (
        typeof parsed.count !== "number" ||
        typeof parsed.firstTs !== "number"
      ) {
        return null;
      }
      return parsed;
    } catch (err) {
      return null;
    }
  };
  const writeCrashReloads = (value) => {
    try {
      window.sessionStorage?.setItem(
        CRASH_RELOADS_KEY,
        JSON.stringify(value)
      );
    } catch (err) {}
  };
  const clearCrashReloads = () => {
    try {
      window.sessionStorage?.removeItem(CRASH_RELOADS_KEY);
    } catch (err) {}
  };
  // The game is ready: the bar fills its last stretch, then the shell
  // fades (the game draws on under it, its screens coming in as the
  // fade begins: the ms until then are returned); the art stops once
  // the shell has faded.
  const ready = () => {
    state.ready = true;
    const fill = art() ? art().finish() : 0;
    window.setTimeout(applyState, fill);
    window.setTimeout(() => art() && art().stop(), fill + 400);
    // A session that stays healthy for a while earns fresh crash-reload
    // budget; a boot-loop burns through it and lands on the overlay.
    window.setTimeout(clearCrashReloads, CRASH_COUNTER_RESET_MS);
    return fill;
  };
  const showCrashOverlay = (message, hint) => {
    state.ready = false;
    state.blocked = { code: "crashed", message, hint: hint || "" };
    const root = resolveDom();
    if (root && root.shell) {
      // applyState never removes is-ready, and is-ready keeps the
      // shell invisible and click-through; drop it explicitly so the
      // overlay shows and blocks input into the dead app.
      root.shell.classList.remove("is-ready");
    }
    applyState();
  };
  // A crashed wasm instance cannot be resumed; recovery is teardown +
  // reload. Reload automatically a couple of times, then fall back to
  // a manual prompt so a crash-on-boot cannot loop forever.
  const crash = (message) => {
    if (state.crashed) {
      return;
    }
    state.crashed = true;
    window.__DDI_CRASHED = true;
    const summary =
      typeof message === "string" && message
        ? message
        : "The game hit an unexpected error.";
    const now = Date.now();
    let reloads = readCrashReloads();
    if (!reloads || now - reloads.firstTs > CRASH_WINDOW_MS) {
      reloads = { count: 0, firstTs: now };
    }
    if (reloads.count < CRASH_MAX_RELOADS) {
      reloads.count += 1;
      writeCrashReloads(reloads);
      showCrashOverlay("Something went wrong — reloading…", summary);
      window.setTimeout(() => window.location.reload(), 300);
    } else {
      showCrashOverlay(
        "Something went wrong. Reload the page to continue.",
        summary
      );
    }
  };
  // Catches raw wasm traps (RuntimeError: unreachable / index out of
  // bounds / …) that escape without going through the Rust panic hook.
  window.addEventListener("error", (event) => {
    const err = event.error;
    const message = event.message || "";
    const isTrap =
      (typeof WebAssembly === "object" &&
        err instanceof WebAssembly.RuntimeError) ||
      /RuntimeError.*(unreachable|index out of bounds|call_indirect)/i.test(
        message
      );
    if (isTrap) {
      crash(message || "WebAssembly runtime error");
    }
  });
  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    if (
      typeof WebAssembly === "object" &&
      reason instanceof WebAssembly.RuntimeError
    ) {
      crash(String(reason && reason.message ? reason.message : reason));
    }
  });
  window.__DDI_BOOT = {
    // The link as the download measured it, bytes a second (0: not yet),
    // and the download's share of the bar (for the verify skill).
    get bandwidth() {
      return bandwidth;
    },
    get share() {
      return share;
    },
    setProgress,
    setStartProgress,
    fail,
    ready,
    crash,
  };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", applyState, { once: true });
  } else {
    applyState();
  }
  const checkCompatibility = () => {
    if (typeof WebAssembly !== "object") {
      return {
        ok: false,
        code: "no-wasm",
        message: "Your browser doesn't support WebAssembly.",
        hint: "Try an up-to-date browser.",
      };
    }
    // The game is built with wasm SIMD (.cargo/config.toml): a v128 const.
    if (!WebAssembly.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11]))) {
      return {
        ok: false,
        code: "no-wasm-simd",
        message: "Your browser doesn't support WebAssembly SIMD.",
        hint: "Try an up-to-date browser (Safari 16.4 or later).",
      };
    }
    if (typeof window.fetch !== "function" || typeof Promise !== "function") {
      return {
        ok: false,
        code: "no-fetch",
        message: "Required web APIs are missing.",
        hint: "Try an up-to-date browser.",
      };
    }
    if (typeof window.TextDecoder !== "function") {
      return {
        ok: false,
        code: "no-textdecoder",
        message: "TextDecoder is required for the game.",
        hint: "Try an up-to-date browser.",
      };
    }
    if (typeof window.requestAnimationFrame !== "function") {
      return {
        ok: false,
        code: "no-raf",
        message: "requestAnimationFrame is required for rendering.",
        hint: "Try an up-to-date browser.",
      };
    }
    if (typeof window.PointerEvent !== "function") {
      return {
        ok: false,
        code: "no-pointer",
        message: "Pointer Events are required for input.",
        hint: "Try a different browser or update it.",
      };
    }
    if (
      typeof window.AudioContext !== "function" &&
      typeof window.webkitAudioContext !== "function"
    ) {
      return {
        ok: false,
        code: "no-audio",
        message: "Web Audio is required — this game is played by sound.",
        hint: "Try an up-to-date browser.",
      };
    }
    return { ok: true };
  };
  const normalizeProgress = (progress, fallbackTotal) => {
    if (typeof progress === "number") {
      return { loaded: progress, total: fallbackTotal || 0 };
    }
    if (!progress || typeof progress !== "object") {
      return null;
    }
    const loaded =
      progress.loaded ??
      progress.current ??
      progress.received ??
      progress.bytes ??
      progress.position;
    const total =
      progress.total ??
      progress.size ??
      progress.length ??
      progress.expected;
    if (typeof loaded !== "number" || typeof total !== "number") {
      return null;
    }
    return { loaded, total };
  };
  window.__DDI_TRUNK_INIT = () => {
    let downloadCompleted = false;
    return {
      onStart: () => {
        const compat = checkCompatibility();
        if (!compat.ok) {
          fail(compat.code, compat.message, compat.hint);
          throw new Error(compat.code || "incompatible");
        }
        setProgress(0);
      },
      onProgress: (...args) => {
        const [first, second] = args;
        let progress = null;
        if (typeof first === "number" && typeof second === "number") {
          progress = { loaded: first, total: second };
        } else {
          progress = normalizeProgress(first, second);
        }
        if (progress && progress.total > 0) {
          const ratio = progress.loaded / progress.total;
          setDownloadProgress(ratio, progress.loaded, progress.total);
          if (!downloadCompleted && ratio >= 1) {
            downloadCompleted = true;
          }
        } else {
          setDownloadProgress(null);
        }
      },
      onComplete: () => {
        if (!downloadCompleted) {
          downloadCompleted = true;
          setDownloadProgress(1);
        }
      },
      onSuccess: () => {},
      onFailure: (error) => {
        const detail =
          error && typeof error.message === "string"
            ? error.message
            : "Initialization failed.";
        fail(
          "init-failed",
          "Something went wrong while starting the game.",
          detail
        );
      },
    };
  };
})();
