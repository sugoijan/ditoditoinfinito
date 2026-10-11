# Running untrusted Lua: what the VM must enforce, which VM, what animations need (2026-10-11)

Scope: StepMania noteskins (and, later, song background animations) are Lua
programs from wherever the player got them. The requirement is that a
player never has to trust a script: none may freeze the game, exhaust
memory or crash it, at any cost in performance. The same guarantees must
hold in the browser and in a native desktop build, so they cannot rest on
the browser (a worker, a separate wasm instance): **the VM itself is the
sandbox.** This note follows `docs/research/noteskins.md` (sections 8–9:
how much Lua skins use, the first look at runtimes). Experiments ran under
a memory watchdog in scratch directories; nothing from a pack was executed.

Sources, pinned: rilua v0.1.27 (`6ee1e4ffdbeeede4c26944524e7541f548ffec05`,
https://github.com/wowemulation-dev/rilua); piccolo 0.3.3 and its trunk
(`ce709eb`, 2025-07-10, https://github.com/kyren/piccolo); PUC Lua 5.1.5 as
StepMania vendors it; StepMania 5.1 (`5_1-new` @ 825467b) and ITGmania
(`release` @ 8a3bf88); Chromium (`worker_thread.cc`), Firefox
(`WorkerPrivate.cpp`) and WebKit (`WorkerOrWorkletThread.cpp`) at their
trunks of 2026-10-11; Rust 1.99.0.

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

## 2. What an in-process sandbox needs, in Rust

The game and the VM share one process (and, on the web, one wasm
instance), so each failure the browser would have contained must be
impossible by construction:

- **Panics cannot be caught on the web.** Release builds use
  `panic = "abort"`; measured, a trap leaves Rust state mid-operation and
  every later call traps again, so the only recovery is a reload. Building
  with `panic = "unwind"` does not help on stable Rust: a
  `wasm32-unknown-unknown` cdylib with `catch_unwind` around a `panic!`
  built with 1.99.0 and still trapped (`unreachable`), because the
  prebuilt standard library aborts. Natively, `catch_unwind` works and is a
  backstop, not a guarantee. **The VM must not panic**: no `unwrap`,
  `expect`, unchecked indexing or arithmetic that can overflow, and no
  unbounded recursion on the native stack.
- **Running out of memory cannot be caught either.** Rust's global
  allocator aborts on failure (`handle_alloc_error`); the hooks that would
  turn it into an error (`set_alloc_error_hook`, `-Z oom=panic`) are
  unstable, and a panic would not be caught anyway. A cap therefore only
  holds if **every allocation the VM makes is charged to its budget before
  it is made**, with fallible allocation (`try_reserve`) for the rest, and
  the cap is well below what the process can get. A limit checked after
  allocating (rilua's, piccolo's) is no cap: one instruction can allocate
  gigabytes first.
- **Nothing can interrupt a long native call.** A thread cannot be killed
  safely, and on the web nothing else runs on the main thread. So **every
  operation must charge its cost before doing the work**: per instruction,
  and inside library functions in proportion to the bytes, elements or
  backtracking steps they process (`string.rep`, `..`, `table.concat`,
  pattern matching, `table.sort`), with the budget and a clock checked at
  short intervals. Then the longest stretch without a check is bounded by
  design, not by what a script chooses.
- **Memory safety.** A VM in C (PUC Lua, Luau) has caps that hold, but a
  bug in it is a memory corruption in the game's process, worse than a
  crash on the desktop; and neither builds for `wasm32-unknown-unknown`
  (section 9 of the noteskin research). In Rust, `unsafe` confined to a
  few reviewed sites (a GC, a hash table) keeps the risk of memory errors
  there and makes the remaining risk a panic, which the points above rule
  out and fuzzing checks.
- **No escape hatches.** No bytecode loader, no file or OS access beyond
  the skin's own files through the host, no `debug`, no finalizers, no
  global state shared between VMs; dropping a VM frees everything it made.

Process isolation on the desktop (a helper process killed on a deadline)
and a worker on the web would add a second layer, but each is a platform of
its own and the requirement is that the guarantees hold without them.

## 3. Which VM

- **rilua** (Lua 5.1, the closest semantics): about 40,000 lines; about 700
  places grow a `Vec` or `String` and none is fallible; about 160 `unwrap`,
  `expect`, `panic!` or `unreachable!` outside tests; Lua → Rust → Lua
  calls recurse on the native stack. Bringing it to the requirements above
  is a rewrite of most of it, kept in sync with an upstream that does not
  share the goal.
- **piccolo** (pure Rust, about 18,600 lines; "resilient against DoS from
  untrusted scripts" is a stated goal). Its design is what the requirements
  ask for: it is stackless, so Lua → Rust → Lua nesting (`pcall`,
  metamethods, callbacks that call Lua) never recurses on the native stack,
  and every step returns to the host with a fuel counter, so a load can
  also be spread over frames. Measured at trunk (`ce709eb`), 1,000 fuel a
  step, a 64 MiB cap checked between steps (the 0.3.3 release is older:
  there `table.unpack({}, 1, 1e8)` reached 1.5 GB in one step):
  - Works: `while true do end`, a spinning coroutine and an endless
    `__tostring` recursion returned to the host every step (2.2 million
    steps in 20 s, about 9 µs each); a table growing to 10⁹ entries, `pcall`
    and coroutine recursion, `__index` and `__concat` recursion and
    `table.unpack({}, 1, 1e8)` stopped near the cap (overshoots up to
    112 MiB, from a stack doubling); all 521 distinct files of the skin
    corpus, `cmd` rewritten, compile; deep parentheses, unary chains,
    nested tables and functions end in "recursion limit reached".
  - Crashes: `a.b.b…` 100,000 deep overflows the native stack while
    compiling (an abort natively too); `math.mininteger % -1` panics at run
    time (`constant.rs:154`, a remainder overflow, also when folding
    constants).
  - Escapes: work proportional to a size is charged neither fuel nor
    memory. `s = s .. s` forty times passed 3 GB inside one step (killed
    by the watchdog); one step took 42 ms to build a 128 MiB string, 24 ms
    for `upper` on 32 MiB, 25 ms for a table rehash.
  - Missing for skins: a string metatable (`s:lower()` fails; about 17 of
    the 521 files call string methods), `string.find`/`gsub`/`match`/
    `format`/`rep`, `xpcall`, `error` levels. Numbers follow Lua 5.4 with an
    integer subtype and print in Rust's shortest form (`1/3` is
    `0.3333333333333333`, `1e15` is `1000000000000000`; Lua 5.1 prints
    `%.14g`: `0.33333333333333`, `1e+15`).
  - Its `unsafe` (32 sites, in tables, strings, callbacks and userdata) and
    gc-arena's are what memory safety rests on. Upstream is quiet (last
    commit 2025-07-10, gc-arena pinned to an unreleased git revision).
- **PUC Lua or Luau in C/C++**: ruled out above (memory safety in-process,
  no `wasm32-unknown-unknown` build).
- **A VM of our own**: Lua 5.1 in safe Rust, written for section 2 from the
  start; roughly 10,000 lines, and a stackless design like piccolo's would
  have to be rebuilt.

**Recommendation: a fork of piccolo.** Its escapes are all in its core
(size-proportional work, an integer panic, a missing recursion guard,
string metatables, number printing), so they cannot be fixed from outside its
API, but they are local patches, not a redesign: charge fuel and memory
before size-dependent work (concatenation, string results, table growth and
rehash, stack growth, constructors); with that, the fixed-size allocations
of one fuel unit bound the overshoot. Everything else (the string library
with patterns and `format`, Lua 5.1's extra functions, the host API) is
written on top of its public API, as callbacks and sequences that charge
the same fuel. The fork is pinned by revision with its patches listed;
they suit upstream's own goals and can be offered there. PUC Lua 5.1.5
stays the reference and the oracle for differential tests.

## 4. Where the VM runs

In the game's process on every platform, called synchronously: a command
runs in the frame its event happens, as StepMania runs it, and its result is
drawn that frame. Each call gets a fuel budget and a deadline; a load is
spread over frames (the VM is stackless) so a loading screen keeps
drawing.

For the record, the browser-only design considered first, a dedicated
worker with its own wasm instance and a fixed maximum memory, measured in
Chrome 154: `terminate()` stopped a worker spinning in wasm after 2.0 s
every time (`kForcibleTerminationDelay`, `worker_thread.cc:84`) while the
page's frames stayed at 16.5–16.8 ms; a new worker was ready in 3.7 ms; a
`postMessage` round trip took 0.4 ms (p50), so replies were one frame late
by construction. Synchronous calls to a worker are not available on the
published site (`SharedArrayBuffer` needs cross-origin isolation, which
GitHub Pages cannot send). It was set aside because a native build has no
equivalent, and an in-process VM also removes the frame of latency.

## 5. How much Lua runs during play

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
uses the message bus, preferences or player options during play. With the
VM in-process, live Lua needs no special path; the classification sizes
the per-frame budget (one update function per receptor each frame), and
recording is an optimisation to keep for later if that cost matters.

## 6. Scripted backgrounds and modfiles

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

## 7. Conclusions for the design

1. The VM is the sandbox, on every platform: it runs in the game's process,
   and its limits are properties of its own code, not of a worker or a wasm
   instance.
2. No existing VM meets that as it is (section 3); piccolo's stackless,
   fuel-driven design does once its core charges size-dependent work
   before doing it and cannot panic. The recommendation is a fork of
   piccolo with those patches and Lua 5.1's behaviour where skins see it,
   tested against PUC Lua and fuzzed with budgets, natively and in wasm.
3. Every call into it has a fuel budget and a deadline, and a load can be
   spread over frames; any failure is a Lua error that turns that skin's
   Lua off; dropping the VM frees all it allocated.
4. Calls are synchronous, so skin commands run in the frame of their event;
   recording commands into native ops is an optimisation for later.
5. Backgrounds get a VM of their own, so a failing background never turns
   off the arrows; modfiles are a separate, much larger project.
