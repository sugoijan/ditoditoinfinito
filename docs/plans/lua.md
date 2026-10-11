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

Proposed bounds, to be set from measurements in steps 3 and 4: during play
1 ms and a matching fuel budget per frame for all of a skin's Lua; at load
a fuel budget per frame of the loading screen and 5 s per skin; 32 MiB per
skin; source 1 MB per file; at most 4,096 actors a skin.

## Step 0. Conventions

- The fork: piccolo at trunk (`ce709eb`), forked by the maintainer on
  GitHub; the workspace depends on it by git revision (gc-arena stays at
  the revision piccolo pins). Its work follows
  [`piccolo-fork.md`](piccolo-fork.md), a self-contained brief with a
  definition of done, and runs in parallel with steps 2 and 3. Upstream
  reports or pull requests only with the maintainer's go-ahead.
- New crate `lua/` (`ddi-lua`) over piccolo: platform-free and with no
  dependencies on the game's crates. Its API: compile a chunk, run a
  function step by step with a `Budget` (fuel per call, the fork's
  `Limits`, a deadline checked between steps against a clock from the
  host), host functions registered as callbacks that charge the same fuel.
  It holds what StepMania adds on top of Lua: the `cmd` rewrite, the
  sandbox's globals, Lua 5.1's aliases.
- The noteskin host API (`Def.*`, `NOTESKIN`, `GAMESTATE`, …) lives in
  `ddi-noteskin` behind a module of its own, over `ddi-lua`.
- Semantics follow PUC Lua 5.1.5 as StepMania vendors it (MIT; `cmd` as a
  keyword), with the fork's `Lua51` setting on.
- Every adversarial case runs natively and as wasm under Node, under the
  memory watchdog; third-party skins stay local.

## Step 1. The fork (in parallel)

Done when the brief's definition of done holds and the maintainer has
pushed it: limits checked before size-dependent allocations and charged as
fuel, the crashes fixed and panicking constructs denied, the string library
and the rest of Lua 5.1's behaviour, the limit cases, the Lua 5.1 outputs
and fuzzing, natively and in wasm.

**Done 2026-10-11**: two rounds (the brief and its second round, R1–R7),
pushed to `sugoijan/piccolo`, branch `hardening`, at `c8a4b77`; the
workspace depends on that revision. Checked here on it: piccolo's suite,
the limit cases' outcomes and CPU-time gates natively and in wasm, and
every case again through `ddi-lua`. Open: R8 (`%s` with a width or
precision scans its argument uncharged, older than both rounds), and the
wall-time gates, to rerun on an idle machine.

## Step 2. `ddi-lua` (commit 2, in parallel with step 1)

- Written against upstream trunk (`ce709eb`), which has the same API less
  `Limits`: the budget and stepping API, the `cmd` rewrite (the research's
  token rewriter, ported, line numbers kept), the sandbox's globals (only
  what is registered: no `io`, `os`, loaders, `debug` or `collectgarbage`;
  `loadfile` comes from the host), Lua 5.1's aliases (`unpack`,
  `table.getn`, `math.mod`, `math.pow`, …).
- Tests: the rewrite on synthetic sources (nested `cmd`, strings and
  comments containing `cmd(`, line numbers in errors); a skin-shaped
  script stepped to completion under a budget; the deadline stopping an
  endless loop. Locally, the corpus's 521 files compile after the rewrite.
- When the fork is done: the dependency moves to its revision, `Limits`
  joins the budget, and the brief's limit cases run again through
  `ddi-lua` (natively and in wasm), with the deadline as the host's stop.

**Done 2026-10-11**, first on the fork's first round, whose review and
these runs found ways past the guarantees (a `__call` chain, colliding
interned strings, garbage counted against the cap, the pattern matcher's
cap) that became the brief's second round; then on the pushed revision,
where the 120 limit cases end as expected natively and in wasm. What it
is:

- `ddi_lua::Vm` (`lua/src/lib.rs`): one skin's state, built from
  `Lua::empty()` with the host's hash seed (`Settings::hash_seed`: the app
  must give each VM one from the platform's randomness, or strings can be
  built to collide), `Compat::Lua51`, the host's `math.random` seed and
  `Limits` (32 MiB, strings of 1 MiB). `Vm::load` takes at most 1 MB of
  source, rewrites `cmd`, and compiles with a work cap (2,000,000 units of
  the fork's `CompileBudget`); synthetic 1 MB sources stop earlier, at the
  memory limit (the compiler counts 384 bytes a token), after 12–32 ms,
  and the corpus's largest file (17 KB) compiles in 0.85 ms; `Vm::run` steps a `Task` 1,000 fuel at a time under
  a `Budget` (fuel for the call, a deadline on the host's clock) and ends
  `Done`, `Paused` (fuel spent: a load goes on next frame) or `Failed`
  (`Compile`, `Error`, `Limit`, `Deadline`). An uncaught error's message is
  made without running Lua (a looping `__tostring` cannot hang the report).
- `cmd` (`lua/src/cmd.rs`): tokens replaced in place, so lines and errors
  match StepMania's; `;`-separated parts, empty parts, `...`, nested `cmd`,
  and function arguments whose bodies contain `;` (block keywords are
  tracked) as `lparser.c`'s `cmd_body`; malformed `cmd(...)` gives
  StepMania's messages.
- The globals (`lua/src/sandbox.rs`): base, `string`, `table`, `math`,
  `coroutine`; `load` and `collectgarbage` removed; `_G`, `_VERSION` "Lua
  5.1", `unpack`, `table.getn`, `math.mod`, `math.pow`, `string.gfind`.
  `print`, `loadfile` and the rest come with the host API (step 3).
- Checked: unit and VM tests (`cargo test -p ddi-lua`); 658 distinct `.lua`
  files of the local skin corpus compile in 40 ms in all, except three that
  StepMania rejects too (a UTF-8 byte order mark, which `luaL_loadbuffer`
  does not skip, and two syntax errors, in the abandoned 5.1 "newskin"
  format and a `Docs` file); the fork's 103 limit cases through a `Vm`,
  natively in release (every outcome as expected, eleven of them differing
  from the fork's default mode because of Lua 5.1 and the missing `load`,
  listed in `lua/tests/local.rs`) and in wasm under Node (no trap, memory
  growth at most 38.4 MiB).

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
