# Implementation plan: running noteskins' Lua safely (2026-10-11)

Run the Lua of imported StepMania noteskins (`docs/plans/noteskins.md`,
step 7) under the rule that a player never has to trust a script, the same
in the browser and in a native desktop build. Research:
`docs/research/lua-sandbox.md` (what an in-process sandbox needs, why a
hardened fork of piccolo, how much Lua runs during play) and `docs/research/noteskins.md`
(section 8: how skins use Lua).

## Guarantees

These come first; performance is traded for them. They are properties of
the VM's own code: nothing relies on a worker, a separate wasm instance or
a separate process.

- **No freeze.** Every call into Lua has a fuel budget and a wall-clock
  deadline. Fuel is charged before the work it pays for: per instruction,
  and inside library and host functions in proportion to the bytes,
  elements or backtracking steps they process, so an operation that would
  overrun is refused before it starts. The budget and the clock are checked
  at least every few hundred fuel units, and no single unit of work is
  unbounded, so a call returns within its deadline plus a bounded overshoot.
  The VM is stackless (every step returns to the host), so a load is spread
  over frames and the loading screen keeps drawing.
- **No memory growth beyond a cap.** Every allocation whose size a script
  controls (concatenation, string results, table growth and rehash, the
  Lua stacks, constructors, library buffers) is checked against the VM's
  budget before it is made; past the cap, after a collection, it is a Lua
  error. What one fuel unit allocates otherwise is small and fixed, so the
  overshoot between checks is bounded too. The cap is far below what the
  game can get. Dropping a VM frees everything it made; VMs share
  nothing.
- **No crash.** The VM does not panic: in the fork and in our code, lints
  deny `unwrap`, `expect`, unchecked indexing and overflowing arithmetic
  (each remaining site justified); Lua → Rust → Lua nesting never recurses
  on the native stack (piccolo is stackless); every recursion left (the
  parser, compiler, pattern matcher) stops at a fixed depth tested to fit
  the wasm stack. `unsafe` stays confined to piccolo's and gc-arena's
  existing sites, which our patches do not touch. This is checked by
  fuzzing with budgets, natively and in wasm. Natively a `catch_unwind`
  around every call is a backstop; the web has none (stable Rust cannot
  catch panics in `wasm32-unknown-unknown`, research section 2), which is
  why panics must be impossible rather than caught.
- **No reach.** Source only (no bytecode format exists), no `debug`, `os`
  or `io`, no loaders but the host's (`loadfile` reads the skin's own
  files), no finalizers; scripts see the host API and nothing else.
- **A clear fallback.** On any failure (a Lua error, the budget, the
  deadline or the memory cap reached, a host limit such as too many actors)
  the skin's VM is dropped, its Lua is off for the rest of the play, its Lua
  elements use the built-in arrows (the per-element fallback that already
  exists), and the results and the import report say so. The next play
  retries once; a second failure marks the skin broken until it is imported
  again.
- **The same everywhere.** One implementation; fuel, not time, decides in
  normal runs, so a skin behaves the same on every platform. The deadline
  is only the backstop for slow machines.

Proposed bounds, to be set from measurements in steps 1 and 4: during play
1 ms and a matching fuel budget per frame for all of a skin's Lua; at load
a fuel budget per frame of the loading screen and 5 s per skin; 32 MiB per
skin; source 1 MB per file; at most 4,096 actors a skin.

## Step 0. Conventions

- The fork: piccolo at trunk (`ce709eb`) with gc-arena at the revision it
  pins, vendored under `vendor/piccolo/` and `vendor/gc-arena/` (MIT; their
  licence files kept, `REUSE.toml` updated), each patch a commit of ours
  listed in a `PATCHES.md` next to it. Upstream reports or pull requests
  only with the maintainer's go-ahead.
- New crate `lua/` (`ddi-lua`) over the fork: platform-free and with no
  dependencies on the game's crates. Its API: compile a chunk, run a
  function step by step with a `Budget` (fuel, memory cap, a clock from the
  host), host functions registered as callbacks that charge the same
  budget. It holds everything that can live on top of piccolo's API: the
  `cmd` rewrite, the libraries and Lua 5.1's behaviour.
- The noteskin host API (`Def.*`, `NOTESKIN`, `GAMESTATE`, …) lives in
  `ddi-noteskin` behind a module of its own, over `ddi-lua`.
- Semantics follow PUC Lua 5.1.5 as StepMania vendors it (MIT; `cmd` as a
  keyword). A local differential test runs every case and the corpus's
  syntax through a PUC `lua5.1` when one is installed, like the chart
  importers' oracle tests; expected outputs are committed so CI needs no
  oracle.
- Every adversarial case lives in the crate's tests and runs natively and as
  wasm under Node (to check the stack depths and that nothing traps), all
  under the memory watchdog. A fuzz target (scripts generated from a
  grammar, plus mutated corpus syntax) runs locally with budgets; its
  findings become tests.

## Step 1. The fork's core (commit 1)

- Vendor the fork and patch its core: a memory budget in the context,
  checked before every size-dependent allocation, and fuel charged by size
  for the same operations (concatenation, string results, table growth and
  rehash, stack growth, constructors); the two crashes found (the `a.b.b…`
  recursion guard, integer `%` and `//` overflow at run time and when
  folding constants); a string metatable; numbers printed as `%.14g`; a
  spent budget or deadline as an error `pcall` cannot catch, so a script
  cannot swallow its own abort. Then the lint pass over its panicking sites.
- `ddi-lua`'s budget and stepping API, the `cmd` rewrite, the sandbox's
  globals (only what we register; no `io`, loaders or `debug`).
- Tests: every adversarial case of the research, natively and as wasm under
  Node, each ending in a clean error within its bound (endless loops,
  `pcall` and coroutine loops, deep recursion and nesting, long operator
  chains, growing tables and strings, `..` doubling, huge sources, integer
  edge cases); the measured overshoot of the deadline and the cap; the
  recursion depths in wasm; the corpus's 521 files compiling.

## Step 2. The libraries (commit 2)

- On top of piccolo's API, as callbacks and sequences that charge fuel:
  string (`find`, `match`, `gmatch`, `gsub` with patterns charged per step
  and depth-limited, `format` with PUC Lua's limits, `rep`, `byte`, `char`
  with result sizes charged up front), `xpcall` and `error` levels,
  Lua 5.1's names (`unpack`, `table.getn`, `math.mod`, …), a per-VM seeded
  `math.random`; piccolo's own base, table and math functions checked
  against PUC Lua and fixed where skins would see a difference.
- Tests: against PUC Lua's outputs; the research's library cases
  (pathological patterns, `string.rep` of 1.5 GB, `unpack({}, 1, 1e8)`,
  `%99999d`); a fuzzing round over the whole VM.

## Step 3. The host API and loading skins (commit 3)

- A prelude of our own modelled on StepMania's MIT `_fallback` scripts:
  `Def.*`, `LoadActor`, the `..` merge, `color`, the alignment and enum
  globals, the Lua-defined actor helpers (`smooth`, `thump`, …).
- `NOTESKIN:GetPath`/`GetMetric*`/`LoadActor` over the skin's files (the
  existing `ddi_noteskin::Skin`), `loadfile` over them, `Var` (Button,
  Element, Player, SpriteOnly, Color, Controller), a `GAMESTATE` subset
  (game, style, current steps' timing, song position, lead-in, player
  options as far as skins read them), `PREFSMAN:GetPreference`,
  `ProductFamily()` as "ITGmania". Anything else is nil and fails loudly.
  Host functions charge the budget and count actors and textures.
- The `NoteSkin.lua` chain and `Load()` per element produce actor trees fed
  to the existing builder; a skin's own redirects and blanks now apply;
  metrics commands that are Lua expressions evaluate.
- Measured natively on the local corpus (third-party skins stay local): load
  time, fuel and heap peak per skin, to set the load bounds and the cap.

## Step 4. Lua during play (commit 4)

- Commands and update functions run in the frame their event happens, from
  the actor runtime, under the per-frame budget; actor methods called from
  Lua are the native runtime's, so getters answer what is drawn.
- The failure handling and reports of the guarantees, in the app.
- Measured: Lua cost per frame at 60 and 144 Hz on the corpus's live skins
  (Schneider's update functions, explosion round robins), on a desktop and a
  phone, to set the per-frame budget.

## Step 5. Documentation and the first bundled skin

`docs/PLAN.md` (a decision for script safety), `AGENTS.md`, the verify
skill. Delta (CC BY 3.0, StepMania 5.1) bundled with its `PROVENANCE.toml`
once it draws as in StepMania; Apache-2.0 skins (Alpha-Arrows, Dake) if
the maintainer accepts that licence for art.

## Later

- Recording commands that need no live Lua into native ops (69 % of skins,
  research section 5), if the per-frame cost asks for it.
- MilkShape 3D models (65 % of skins, every classic ITG one).
- Scripted song backgrounds: a VM of their own (a failing background never
  turns off the arrows), StepMania's effect and transition files, `Quad`,
  theme fonts by stand-in, render targets and geometry capped.
- Modfiles (scripts that move the playfield and change the game) are a
  separate project.
