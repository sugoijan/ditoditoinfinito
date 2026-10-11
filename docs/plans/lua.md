# Implementation plan: running noteskins' Lua safely (2026-10-11)

Run the Lua of imported StepMania noteskins (`docs/plans/noteskins.md`,
step 7) under the rule that a player never has to trust a script.
Research: `docs/research/lua-sandbox.md` (what the VM enforces, where it
runs, what animations need) and `docs/research/noteskins.md` (sections
8–9: how skins use Lua, the choice of rilua).

## Guarantees

These come first; performance is traded for them.

- **No freeze.** No skin code runs on the main thread, at load or during
  play. The main thread's skin work is native and capped per frame (the
  actor runtime's caps, plus at most K ops applied from a worker reply). A
  slow or hung script stalls only the skin's own live animation, for at
  most T_hang, then the built-in arrows take over for that skin's Lua
  elements.
- **No memory growth beyond a cap.** The VM and everything a script
  allocates live in a separate wasm instance whose memory has a fixed
  maximum M; it is discarded on a kill. What comes back to the game is
  bounded data (actor trees capped in actors and texture references, ops
  capped in count and bytes).
- **No crash.** Nothing a script does can reach the game's wasm instance: a
  trap or panic in the VM's module is an exception in its worker.
- **A clear fallback.** On any failure (a trap, a Lua error, a budget or the
  memory cap reached, an oversized reply, a missed deadline) the worker is
  terminated, the skin's Lua is off for the rest of the play, its Lua
  elements use the built-in arrows (the per-element fallback that already
  exists), and the results and the import report say so. The next play
  retries once; a second failure marks the skin broken until it is imported
  again.

Proposed bounds, to be set from measurements in steps 2 and 5: T_hang
250 ms, T_load 5 s, M 64 MiB, K 4,096 ops and 256 KB per reply, at most
4,096 actors a skin.

## Step 0. Conventions

- New crate `skinvm/` (`ddi-skinvm`): rilua (pinned), the hardening, the
  host API, the recorder; built natively for tests and as a wasm module run
  in a dedicated worker. It links `ddi-noteskin` (the actor runtime, for
  getters and recording) and `ddi-chart` (timing, for
  `GetBeatFromElapsedTime`).
- The worker's script is embedded in the app and started from a blob URL,
  as the video decoder's is; the module is fetched only when a chosen skin
  has Lua.
- Every adversarial case lives in the crate's tests and runs natively and
  as wasm under Node, all under the memory watchdog.
- Upstream changes to rilua are proposed to its maintainers only with the
  maintainer's go-ahead; until they land, the crate wraps or vendors what it
  needs.

## Step 1. A hardened VM (commit 1)

- Source only: a chunk starting with `0x1B` is refused (no bytecode
  anywhere), sources are size-capped, operator-chain depth is checked before
  parsing (rilua's parser recurses on them).
- StepMania's `cmd(...)` syntax rewritten to closures before parsing (the
  survey's token rewriter, ported; line numbers kept).
- Libraries: base, string, table, math only; `load`, `loadstring`,
  `loadfile`, `dofile`, `newproxy`, `collectgarbage`, `gcinfo`,
  `getfenv`/`setfenv`, `string.dump` removed; coroutines left out (no skin
  uses them) or given the hook; `__metatable` set on every host metatable.
- Budgets: a sticky count hook (period about 100) with an instruction budget
  and a wall-clock deadline per call; argument checks before costly library
  functions (`string.rep` within the remaining memory, `format` widths of
  two digits, `unpack`/`byte`/`char` results, `concat`/`gsub` output size,
  subject and pattern lengths for pattern functions). Host callbacks never
  panic.
- Tests: every case of the sandbox research (endless loops, `pcall` loops,
  coroutines, finalizers, pathological patterns, huge strings and tables,
  deep recursion and nesting, bytecode, panicking formats), natively and as
  wasm, each ending in a clean Lua error or, for what only the worker can
  stop, a documented kill. Plus random-script fuzzing with budgets.

## Step 2. The host API and loading skins (commit 2)

- A prelude of our own modelled on StepMania's MIT `_fallback` scripts:
  `Def.*`, `LoadActor`, the `..` merge, `color`, the alignment and enum
  globals, the Lua-defined actor helpers.
- `NOTESKIN:GetPath`/`GetMetric*`/`LoadActor` over the skin's files (the
  existing `ddi_noteskin::Skin`), `Var` (Button, Element, Player,
  SpriteOnly, Color, Controller), a `GAMESTATE` subset (game, style,
  current steps' timing, song position, lead-in, player options as far as
  skins read them), `PREFSMAN:GetPreference`, `ProductFamily()` as
  "ITGmania". Anything else is nil and fails loudly.
- The `NoteSkin.lua` chain and `Load()` per element produce actor trees as
  data, fed to the existing builder; a skin's own redirects and blanks now
  apply.
- Measured natively on the local corpus (third-party skins stay local): load
  time, instructions and heap peak per skin, to set T_load and M.

## Step 3. Recording what need not stay live (commit 3)

- Each command and update function runs once against a recording actor
  proxy with impure host functions instrumented; the result is a recorded
  op list, a table keyed by its parameters (judgement, bright), or "live".
- Recorded and keyed commands are replayed by the native runtime in the
  same frame, as metrics commands are now. The classification is checked
  against the research's static 69 % / 30 % estimate.

## Step 4. The worker (commit 4)

- The module in its own worker with a fixed maximum memory; load and
  classification there; kill and restart; the failure handling and reports
  of the guarantees.
- A check route (like `#/video-check`) that loads a skin, runs a stress
  script and kills a spinning worker, showing terminate latency, frame
  gaps and round trips: for Firefox and Safari by hand.

## Step 5. Live Lua during play (commit 5)

- For skins with live functions only: each frame the page posts its skin
  events, song time and the predicted time and beat of the next frame; the
  worker keeps a shadow actor runtime fed the same inputs (so getters answer
  what the page drew), runs live commands and update functions, and returns
  capped ops stamped with the times they were due; the page applies them the
  next frame and never waits.
- Measured: Lua cost and ops per frame at 60 and 144 Hz, the cost of applying
  K ops, the prediction error on variable-rate displays; a native test that
  the shadow runtime matches the page's.

## Step 6. Documentation and the first bundled skin

`docs/PLAN.md` (a decision for script safety), `AGENTS.md`, the verify
skill. Delta (CC BY 3.0, StepMania 5.1) bundled with its `PROVENANCE.toml`
once it draws as in StepMania; Apache-2.0 skins (Alpha-Arrows, Dake) if
the maintainer accepts that licence for art.

## Later

- MilkShape 3D models (65 % of skins, every classic ITG one).
- Scripted song backgrounds: the same module in a worker of their own (a
  failing background never turns off the arrows), StepMania's effect and
  transition files, `Quad`, theme fonts by stand-in, render targets and
  geometry capped.
- Modfiles (scripts that move the playfield and change the game) are a
  separate project.
