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
wall-time gates, to rerun on an idle machine. Since 2026-10-12 the
workspace depends on the consolidated branch `hardening2` at `75ab65a`
(`docs/plans/piccolo-fork.md`, "Consolidated").

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
  GameButton, Element, Player, SpriteOnly, Color, Controller), a
  `GAMESTATE` subset (game, style, current steps' timing, song position,
  lead-in, player options as far as skins read them, and OutFox's
  `GetIsFieldReversed`), `PREFSMAN:GetPreference`, `ProductFamily()` as
  "ITGmania", and since 2026-10-12 (`docs/plans/bundled-skins.md`, step 4)
  OutFox's `INPUTFILTER:GameButtonToKeyMapped` (`""`), a `THEME` knowing
  NoteField's `NoteFieldType` and `ShowBoard`, and `_fallback`'s
  `JudgmentLineToColor` in the game's colours. Anything else is nil and
  fails loudly.
  Host functions charge the budget and count actors and textures.
- The `NoteSkin.lua` chain and `Load()` per element produce actor trees fed
  to the existing builder; a skin's own redirects and blanks now apply;
  metrics commands that are Lua expressions evaluate.
- Measured natively on the local corpus (third-party skins stay local): load
  time, fuel and heap peak per skin, to set the load bounds and the cap.

**Done 2026-10-11, natively** (the app does not use it yet). `ddi_noteskin::lua::LuaSkin` loads a skin's Lua and builds
any element StepMania's engine asks for. What differs from the plan, and
what step 4 needs:

- The stand-ins run StepMania's own Lua: `common`'s and `dance/default`'s
  `NoteSkin.lua`, `Fallback Explosion.lua`, `Fallback Tap Mine.lua`,
  `Fallback Receptor.lua` and its helpers, `dance/default`'s tap note, lift
  and receptor, byte for byte under `noteskin/src/builtin/stepmania/`
  (credited in `THIRD_PARTY.toml` with the metrics). Their art stays the
  built-in skin's; the native versions of the same Lua remain the fallback
  when a skin's Lua cannot run.
- Every command of such a skin is a Lua function: metrics commands are
  compiled as `ParseCommandList` writes them (`commands::to_lua`; a metric
  that does not compile is warned about and nil, as in StepMania). They are
  kept in the VM by actor id; actors stay native (`Stage`), and their Lua
  handles (userdata with fields of their own, `Actor`/`Sprite`/
  `ActorFrame`/`Model` method tables skins may extend) call the native
  methods and getters. `playcommand` and `Init` (children first) are
  played by the prelude's driver.
- Def tables are converted as `XNodeFromTable` and the `LoadFromNode`
  functions read them: `Condition`, `Class`/`Type`, `Name`, `Base*`,
  `*Command`, `Texture` (relative to the file that wrote the table, found
  as `Def`'s `debug.getinfo` loop finds it, so past tail calls too),
  `Frames` and `FrameNNNN`/`DelayNNNN`, `children` or the array. A model,
  text, a movie or another unsupported actor makes the element the built-in
  skin's, as before.
- Failures: a Lua error while an element loads makes that element the
  built-in skin's with a note (StepMania shows a blank one and goes on); a
  failing command is reported and the other actors' go on; a table already
  on the way down is skipped (`ProcessedTables`). The memory limit, the
  load's fuel (20 million) or deadline (5 s), or a host limit (4,096
  actors, 200,000 table entries and frames read per element, nesting 32
  deep) turn the whole skin's Lua off. The converter and host functions
  run outside the VM's fuel, so they read strings only up to a bound
  (names and paths 4 KiB, method arguments 256 bytes), read each table once
  per element, stop matching file names once they know enough, keep the
  path cache bounded and index the folders' texts.
- Reviewed independently: it found a panic on paths whose lowercase
  differs in length (KELVIN SIGN, `İ`), a class chain a skin could turn
  into an endless native loop, unbounded recursion in the `cmd` rewrite
  (now 200 levels), host memory and time outside the caps (names, paths,
  `_Source` and `queuecommand` strings copied per actor, cleared arrays,
  the path cache, redirects re-reading every text), and `GetChild`/
  `GetChildren` with children of one name (now StepMania's pass-through
  tables). Each is fixed and in `hostile_skins_end_quickly` or its own
  test; every hostile case ends within 2 s and the VM's 32 MiB.
- ITGmania's `Var "Color"` reads "4th": notes load once, not once per
  quantization colour as ITGmania loads them (STARLiGHT and ITGmania's
  `couples` ask for it). Done since 2026-10-12 (`docs/plans/bundled-skins.md`,
  step 4): a note element whose Lua reads it loads once per note type.
- Checked: unit tests on synthetic skins (the stand-ins, a Delta-like skin,
  fields and added methods, `SetStateProperties`, sandboxing, errors and
  limits, hostile tables and loops, each ending within 2 s); the local
  corpus, `DDI_NOTESKINS_LUA=1`: all 27 dance skins of StepMania 5.1 and
  ITGmania and all 318 community skins whose fallbacks are present load,
  in 8–45 ms, 0.25–0.45 million fuel and under 1 MiB each. The elements
  left to the built-in skin are 3D models (4,734 of about 53,800), legacy
  formats StepMania 5 does not load either (`.model`, `.sprite`, `.xml`:
  4,458) and one skin in the abandoned "newskin" format. The community
  clones hold no images (file lists only), so images are placeholders.
- Left for step 4: building the play's elements through `LuaSkin` (the
  app keeps `.lua` texts with imported skins), playing event commands and
  `queuecommand`s through the driver, update functions
  (`SetUpdateFunction`), the song position during play, and per-colour
  notes.

## Step 4. Lua during play (commit 4)

- Commands and update functions run in the frame their event happens, from
  the actor runtime, under the per-frame budget; actor methods called from
  Lua are the native runtime's, so getters answer what is drawn.
- The failure handling and reports of the guarantees, in the app.
- Measured: Lua cost per frame at 60 and 144 Hz on the corpus's live skins
  (Schneider's update functions, explosion round robins), on a desktop and a
  phone, to set the per-frame budget.

**Done 2026-10-11** (native and headless Chrome; Firefox, Safari and a
phone by hand). What it does, and what differs from the plan:

- The trees stay with the skin's Lua until every element is loaded (an
  element's Load or Init may reach another's actors, as in StepMania);
  then the play holds them (`LuaSkin::reclaim`) and lends them back while
  Lua runs (`LuaSkin::lend`), so there is one copy of each actor and Lua's
  getters read what is drawn. Lua reaching a tree the play holds fails
  visibly ("the actor is gone"). A frame starts with the song's position
  (`GAMESTATE:GetSongPosition()`, `GetSongBeat`, `GetCurBPS` and the
  timing's conversions now follow the chart's own timing) and one budget
  for all of the skin's Lua that frame.
- Events play through the prelude's driver (`playcommand`, children
  after the actor) with `GhostArrowRow`'s `ColumnJudgment` message first
  (`TapNoteScore`/`HoldNoteScore`, `Column`, `Bright`), for native skins
  too; commands the engine plays, queued ones and `Init` get an empty
  table without parameters (StepMania's `Message` always has one), a
  script's own `playcommand` nil. A tree's update is the native one split into steps
  (`Actor::update_begin`, `tween_step`, `update_end`), so a command a
  tween queued plays through Lua at the moment it begins, then frames'
  update functions (`SetUpdateFunction`, frames only as in StepMania)
  after their children with the seconds.
- The budget: 200,000 fuel and 50 ms per frame. Every call into the VM
  costs 200 fuel besides its instructions, and actor methods that walk a
  tree or a sprite's states (`finishtweening`, `hurrytweening`,
  `SetAllStateDelays`) pay per actor, tween and state. Measured on the
  local corpus (`DDI_NOTESKINS_LUA_PLAY=1`, a note every 0.2 s and a jump
  of four every 2 s, 20 s at 60 and 144 Hz): no skin above 49,882 fuel in
  a frame (the jumps), most below 12,000, 0.02–0.06 ms per frame on
  average natively; an endless loop reaches the budget in about 1.5 ms.
  Times on this machine were noisy (the fuzzers ran); a phone is still to
  measure.
- Loads run in slices of 100,000 fuel with the page's turn between them;
  the load's deadline counts the time spent in Lua only. Each play loads
  the skin's Lua twice, as the native build did: once to learn which
  images it asks for, once with their sizes.
- Skins with no Lua of their own (nor in the imported skins they fall
  back on) play natively, as before: the native stand-ins are the same
  Lua's behaviour, without its cost or risks.
- Failures: a command or update function that errors is reported and the
  play goes on. The fuel, the deadline, the memory limit or a host limit
  end the skin's Lua, and every later call fails the same way: at load
  the skin is built without it (natively, its images decoded for that);
  in a play its VM is dropped and every element is the built-in skin's
  for the rest of the play and its retries. The next play tries again
  (the first stop is remembered for the session, and forgotten when the
  skin is imported again); a second stop turns the skin's Lua off until
  it is imported again (`lua_broken` in its record, "its Lua is off"
  under Options). A play stopped by the 50 ms deadline alone is not held
  against the skin (a stalled machine). The import report runs each Lua
  skin's Lua and says how many elements are its own and why the others
  are not, or why its Lua stopped.
- Imports keep `.lua` texts with the skin's record; skins imported before
  get theirs from their stored files when first played.
- Each play's VMs get a hash seed from the platform's randomness
  (`crypto.getRandomValues`), the same for its two loads so they ask for
  the same images.
- A retry restores the actors as built; the Lua's own state (upvalues,
  fields) carries over, where StepMania would load the skin again.
- Reviewed independently (two reviewers): fixed native work bought for a
  few fuel (above), missing empty parameter tables, copies of held trees
  silently taking Lua's calls, a host limit turning into a soft error on
  later calls, strikes surviving a re-import, the native fallback after a
  failed second load missing images, image-only skins exposed to the
  Lua's deadline, blank elements left blank after a stop, records
  rewritten on every play.
- Not done: per-colour notes (ITGmania's `Var "Color"` still reads
  "4th"; done since in `docs/plans/bundled-skins.md`, step 4),
  `ShowNoteUpcoming`/`MinefieldOn`, the per-frame cost on a phone.
  A skin readied for a song the player leaves keeps loading in the
  background (its slices yield) and its stop still counts.
- Checked: unit tests on synthetic skins (events with parameters, a
  round robin re-armed by `queuecommand`, an update function reading the
  song's position, a frame out of fuel, a sliced load) and through the
  renderer (`render/src/noteskin.rs`); the corpus as above; in headless
  Chrome on WebGPU and WebGL2 a live skin (beat flash from an update
  function, explosions coloured by the message's score), a skin whose
  update function loops after 4 s (stops, the built-in skin takes over,
  a second play stops again and the third runs without Lua) and one whose
  `NoteSkin.lua` never returns (the import reports it; it plays
  natively).

## Step 5. Documentation and the first bundled skin

`docs/PLAN.md` (a decision for script safety), `AGENTS.md`, the verify
skill. Delta (CC BY 3.0, StepMania 5.1) bundled with its `PROVENANCE.toml`
once it draws as in StepMania; Apache-2.0 skins (Alpha-Arrows, Dake) if
the maintainer accepts that licence for art.

**Done 2026-10-12**, with `docs/plans/bundled-skins.md`: decision 39 (the
sandbox and its bounds), the docs and the verify skill kept current at
each step, Delta, Alpha-Arrows (its images; our own Lua) and Dake
bundled, and Dito, the built-in arrows as a Lua skin, the default.

## Later

- Recording commands that need no live Lua into native ops (69 % of skins,
  research section 5), if the per-frame cost asks for it.
- MilkShape 3D models (65 % of skins, every classic ITG one).
- Scripted song backgrounds: a VM of their own (a failing background never
  turns off the arrows), StepMania's effect and transition files, `Quad`,
  theme fonts by stand-in, render targets and geometry capped.
- Modfiles (scripts that move the playfield and change the game) are a
  separate project.
