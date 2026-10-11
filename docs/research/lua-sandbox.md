# Running untrusted Lua: what the VM can enforce, where it runs, what animations need (2026-10-11)

Scope: StepMania noteskins (and, later, song background animations) are Lua
programs from wherever the player got them. The requirement is that a
player never has to trust a script: none may freeze the game, exhaust
memory or crash the tab, at any cost in performance. This note follows
`docs/research/noteskins.md` (sections 8–9: how much Lua skins use, the
choice of rilua). Experiments ran under a memory watchdog in scratch
directories; nothing from a pack was executed.

Sources, pinned: rilua v0.1.27 (`6ee1e4ffdbeeede4c26944524e7541f548ffec05`,
https://github.com/wowemulation-dev/rilua); PUC Lua 5.1.5 as StepMania
vendors it; StepMania 5.1 (`5_1-new` @ 825467b) and ITGmania (`release` @
8a3bf88); Chromium (`worker_thread.cc`), Firefox (`WorkerPrivate.cpp`) and
WebKit (`WorkerOrWorkletThread.cpp`) at their trunks of 2026-10-11.

## 1. What rilua enforces in-process

**Works:**
- A count hook can be set from Rust without the `debug` library
  (`state_mut().hook`); returning `Err` unwinds cleanly to the host and the
  state stays usable. Cost: none at a period of 1000 instructions, about
  9 % at 100, 56 % at 10.
- `pcall`/`xpcall` catch every error (no uncatchable variant), but a
  "sticky" hook, which fires on every instruction once the budget is spent,
  aborts nested `pcall` loops and loops inside handlers.
- Built-in limits held in tests: Lua call depth 20,000, native call depth
  200, `__index` chains 100, parser nesting 200, pattern recursion 200; the
  recursion cases (`__index`, `__tostring`, `__concat`) give catchable
  "stack overflow" errors.
- `math.random` is per state, seeded by the host; without `os` there is no
  clock.

**Escapes or breaks:**
- Coroutines start without the hook (PUC Lua copies it to new threads).
- `__gc` finalizers run with hooks off (as in PUC Lua): an endless
  finalizer ran until killed.
- An instruction count is not time: a growing tail call took 26 s for
  400,000 instructions; native library calls never see the hook, so
  `string.find` with a pathological pattern ran past 25 s (PUC Lua: 6.2 s
  with 20 characters and no hook call at all).
- The memory limit (`set_alloc_limit`) is an estimate checked after
  allocating, and the Lua stack and library buffers are not counted: with a
  64 MiB limit, `string.rep("x", 1.5e9)` and `table.concat` went past 2 GB,
  `unpack({}, 1, 1e8)` reached 1.5 GB with the counter unchanged.
- In wasm an allocation failure is a trap, not a Lua error; memory that
  grew is never given back.
- Panics: `string.format("%99999d", 1)`; long operator chains (`1+1+…`,
  `a.b.b…`, about 8 KB of source in wasm) overflow the native stack while
  building the syntax tree.
- Every loader accepts precompiled bytecode with no verifier (a chunk
  starting with `\x1bLua`), including the host API; the base library also
  brings `load`, `loadstring`, `loadfile`, `dofile`, `newproxy`,
  `collectgarbage`, `getfenv`/`setfenv`, and the string library `dump`.
- Panics in host callbacks are not caught; natively some string functions
  follow the C locale while the wasm build does not.

rilua's unsafe code is limited to libc calls, a few `Send`/`Sync` impls and
optional dynamic modules; its VM uses safe arenas. It has about 1,350 tests
and oracle tests against PUC Lua, but no fuzzing with resource budgets in
CI. For comparison, PUC Lua's `lua_Alloc` sees and can refuse every
allocation before it happens, and Luau was designed for untrusted code
(no bytecode loading, sandboxed globals, an interrupt callback at loop
back-edges and calls).

Small upstream patches would close most of the gaps, several as fidelity
fixes: new threads inherit the hook; `format` width and precision limited
as in PUC Lua; result-count checks in `unpack` and `string.byte`; the
allocation limit checked before allocating and covering the stack; bounded
depth for operator chains. An abort `pcall` cannot catch, and step checks
inside the pattern matcher, are new API and need discussion.

## 2. Where the VM runs

- **Inside the game's wasm instance: rejected.** Release builds use
  `panic = "abort"`, so a panic or an out-of-memory abort is a trap that
  leaves Rust state mid-operation: measured, every later call trapped
  again. The only recovery is a fresh instance, which is a reload; the
  game's memory has no maximum and never shrinks; and nothing on the main
  thread can interrupt a long native call. However good the VM's own
  limits, they are only as good as its correctness.
- **A dedicated Web Worker with its own wasm module and a fixed maximum
  memory: recommended.** Measured in Chrome 154:
  - `terminate()` stops a worker spinning in wasm after 2.0 s every time
    (`kForcibleTerminationDelay`, `worker_thread.cc:84`), while the page's
    frames stay at 16.5–16.8 ms, also after ten kills in a row; Firefox
    stops it at the next interrupt check (`WorkerPrivate.cpp`), WebKit
    likewise (`scheduleExecutionTermination`); Firefox and Safari are to be
    measured by hand.
  - A new worker with the compiled module is ready in 3.7 ms.
  - At the cap `memory.grow` fails; traps (`unreachable`, stack overflow,
    out of memory) are caught in the worker, which keeps answering, as the
    video decoder's worker already does.
  - A `postMessage` round trip takes 0.4 ms (p50) with 0.3 ms of work and
    4 KB; every reply arrived before the next frame, so a worker's results
    are one frame late by construction.
- **Synchronous calls are not available on the published site.**
  `Atomics.wait` throws on the main thread, and `SharedArrayBuffer` needs
  cross-origin isolation (COOP/COEP headers), which GitHub Pages cannot
  send; the service-worker workaround (coi-serviceworker) costs two page
  loads on the first visit and after every hard reload, routes every fetch
  through itself, and picks a COEP mode Safari lacks.

## 3. How much Lua must stay live during play

A static classification of every play-time function in the 324 dance
skins of the earlier survey (hand-checked): 225 skins (69 %) only run
commands that can be recorded once or keyed by their parameters, including
every skin bundled with StepMania 5.1, ITGmania and OutFox; 97 (30 %) keep
Lua live, all from four community repositories (Schneider 70, Peter
Lawrence 10, STARLiGHT 9, HURG 8); 2 have no play-time function. The live
causes are few: an update function (87 skins, every one the same receptor
beat flash), state kept on the actor for a round robin of explosions
re-armed by `sleep(4/60):queuecommand("Emit")` (88), `GetHoldLength` or
`GetHeight` (35), upvalues across calls (15), `math.random` (9). No skin
uses the message bus, preferences or player options during play.

## 4. Scripted backgrounds and modfiles

- StepMania's `#BGCHANGES` and `#BGCHANGES2` (`#ANIMATIONS` an alias) name
  a song-folder directory, movie or image, then the global `BGAnimations`,
  `SongMovies`, `RandomMovies` (`BackgroundUtil.cpp`). Even still images go
  through Lua: `BackgroundEffects/<effect>.lua` loads `Var "File1"`; a
  directory loads through its `default.lua`; `default.xml` and
  `BGAnimation.ini` need QuirksMode, off by default. Every background is
  built when the song loads; a change sends `LoseFocus` and the transition
  out, `InitState`, `On`, `GainFocus` in. Foregrounds live only in the song
  folder and as long as their tweens run (scripts `sleep(69420)` to stay).
- Real scripted content is almost all **modfiles** (Lua that moves the
  playfield, swaps charts, implements mechanics): of 25 scripted songs and
  templates found in public repositories (telperion's modulus and
  BTICHES-2, OutFox Serenity, PIU-Simfiles, Hodobox, templates), 23 are
  modfiles, one is a 16-line visual background, one a 26-line background
  reading song state. The maintainer's local library (934 charts) has none.
  NotITG modfiles are another engine (Windows only, GLSL shaders, file
  access) and out of scope.
- Animations use 334 distinct actor methods, 65 of them also in noteskins
  (59 % of call sites), plus `ActorProxy`, render targets
  (`ActorFrameTexture`), `ActorMultiVertex`, `Sound`, theme fonts by name
  (`LoadFont("Common Normal")`, 614 uses), the message bus, update and draw
  functions, the z-buffer and masks, the 3D camera, and 23 engine
  singletons (23 of 25 scripts reach the gameplay screen or player
  options).
- StepMania bounds almost nothing: no instruction limit, render targets of
  any size, unchecked vertex counts (a negative one becomes huge), proxy
  cycles, file writes, screen and preference changes, globals kept across
  songs. A runtime of ours has to bound each.

## 5. Conclusions for the design

1. Skin and animation Lua runs only in a worker, in its own module with a
   fixed maximum memory, and is terminated on any failure or deadline; the
   game never waits for it.
2. Inside that module, the VM is still hardened (source only, a reduced
   library, a sticky count hook plus a clock deadline, argument checks on
   costly library functions, coroutines and finalizers left out or fixed)
   so that most failures are a clean Lua error instead of a kill.
3. Commands that can be recorded at load are replayed by our native actor
   runtime in the same frame, as now; only the 30 % of skins with live
   Lua talk to the worker during play, one frame late.
4. Background scripts can share the module but get their own worker, so a
   failing background never turns off the arrows; modfiles are a separate,
   much larger project.
