# Brief: hardening a fork of piccolo for untrusted scripts (2026-10-11)

A self-contained brief for the work on the fork, done in parallel with the
game. Background: `docs/research/lua-sandbox.md` (sections 2–3) and
`docs/plans/lua.md` in Dito Dito Infinito; nothing here needs them.

## Context

The game runs Lua from StepMania noteskins that players download. No
script may freeze the game, use memory past a cap, or crash it, in the
browser (`wasm32-unknown-unknown`, `panic = "abort"`, where neither a panic
nor a failed allocation can be caught on stable Rust) and in a native
desktop build. The VM therefore has to enforce the limits itself.

piccolo is the base because it is stackless (Lua → Rust → Lua never recurses
on the native stack) and every `Executor::step` returns to the host with a
fuel count. Measured at trunk `ce709eb` (1,000 fuel a step, a 64 MiB cap
checked by the host between steps), what it lacks:

- Size-proportional work is charged neither fuel nor memory: `s = s .. s`
  forty times passed 3 GB inside one step; single steps took 42 ms
  (building a 128 MiB string), 24 ms (`upper` on 32 MiB), 25 ms (a table
  rehash); stack doublings overshot the cap to 112 MiB.
- Crashes: `local a = {} a.b = a local x = a.b.b.b…` (100,000 deep)
  overflows the native stack while compiling; `math.mininteger % -1`
  panics at run time (`src/constant.rs:154`, remainder overflow; also when
  folding constants).
- Missing: a string metatable (`("x"):rep(3)` fails), `string.find`,
  `match`, `gmatch`, `gsub`, `format`, `rep`, `xpcall`, `error` levels.
- Numbers print in Rust's shortest form (`1/3` → `0.3333333333333333`,
  `1e15` → `1000000000000000`); Lua prints `%.14g`.
- `math.random` seeds from entropy (`SmallRng::from_entropy`), which pulls
  `getrandom` and JS imports into a wasm build.

## Ground rules

- Base: `https://github.com/kyren/piccolo` at `ce709eb1dae5c543cbc78e7e12bb80249d88c55f`,
  in the maintainer's fork, on a branch of the maintainer's choosing. Keep
  gc-arena at the revision piccolo pins; if a gate seems to need a gc-arena
  change, stop and report it (the maintainer forks it).
- One commit per patch, each with its tests, message explaining why.
  Commit and push only as the maintainer instructs. Never open issues or
  pull requests upstream (kyren/piccolo, kyren/gc-arena) or comment there.
- No new `unsafe`; do not modify existing `unsafe` blocks except to delete
  them. No new runtime dependencies; dev and fuzz dependencies are fine.
  Latest stable Rust for everything but the fuzz targets (nightly is fine
  there).
- Behaviour that differs between Lua versions goes behind an opt-in (a
  `Lua51` compatibility setting or cargo feature): piccolo's default stays
  as it is, so patches stay offerable upstream later. Safety fixes apply
  always.
- Every ad-hoc binary, test run of the limit cases and benchmark runs under
  a memory and time watchdog (kill above 2 GB RSS or after 60 s), for
  example:

  ```bash
  #!/bin/bash
  # wd.sh <limit_s> cmd args...: kills the command above 2 GB RSS or after limit_s.
  lim=$1; shift; "$@" & pid=$!; start=$(date +%s); maxrss=0
  while kill -0 $pid 2>/dev/null; do
    rss=$(ps -o rss= -p $pid 2>/dev/null | tr -d ' ')
    [ -n "$rss" ] && [ "$rss" -gt "$maxrss" ] && maxrss=$rss
    if [ -n "$rss" ] && [ "$rss" -gt 2000000 ]; then kill -9 $pid; echo "WATCHDOG: rss=${rss}KB"; break; fi
    if [ $(( $(date +%s) - start )) -ge "$lim" ]; then kill -9 $pid; echo "WATCHDOG: ${lim}s"; break; fi
    sleep 0.2
  done
  wait $pid; rc=$?; echo "[wd] rc=$rc maxrss=${maxrss}KB"; exit $rc
  ```

  Fuzzers use libFuzzer's own limits instead (`-rss_limit_mb=2048`,
  `-timeout`, `-max_total_time`).

## Deliverables

### D1. Limits and their API

The host configures, per `Lua` instance (names may change if the report
says so; the game's side is written against these):

```rust
pub struct Limits {
    /// Bytes the instance may hold: everything gc-arena tracks plus the
    /// external allocations piccolo reports (stacks, table storage, buffers).
    pub memory: Option<usize>,
    /// Longest Lua string, in bytes (a string of exactly this length is allowed).
    pub string_len: Option<usize>,
}
impl Lua {
    pub fn set_limits(&mut self, limits: Limits);
}
```

- **Before** any allocation whose size a script controls, piccolo checks it
  against `memory` (and `string_len` for strings) and refuses it if it
  would pass. At least: concatenation (`..` and `__concat` results,
  `table.concat`), every string a library function returns, table array
  and hash growth and rehash, constructors and `SETLIST`, Lua stack and
  call frame growth, coroutine creation, varargs and `unpack`-style
  results, the compiler's buffers. Fixed-size allocations need no check:
  the overshoot is bounded by what one step can allocate.
- A refused allocation ends the executor with an error that `pcall`,
  `xpcall` and coroutines cannot catch, that the host can tell apart
  (for example `piccolo::LimitError::{Memory, StringLength}` reachable by
  downcast from the executor's error). After it, the `Lua` instance may be
  dropped; any further use must still not panic.
- Converting an error value to a message on the host side never runs Lua
  (no `__tostring` metamethods).

### D2. Fuel in proportion to work

- Every operation whose time depends on a size charges fuel for that size,
  before doing the work: concatenation, string library functions (per byte
  read and written), `table.concat`, `table.insert`/`remove` shifts,
  `unpack`, `select`, `#` on tables (the border search), table growth and
  rehash, `sort` (per comparison), `tostring`/`tonumber` on long inputs,
  compilation (per source byte).
- Pattern matching charges fuel per matcher step, and a match exceeding a
  fixed step cap (documented; on the order of 10⁶) or the recursion cap
  (200, as in Lua's `MAXCCALLS`) is a Lua error; or the matcher is made a
  resumable sequence, either way it may not run unbounded within a step.
- Fuel may go negative within an operation (the step ends after it). The
  cost model (fuel per instruction and per byte, element, comparison or
  matcher step for each operation) is written down in `PATCHES.md`.

### D3. No crashes

- Fix the two crashes above, in the run-time and constant-folding paths
  (integer `%` and `//` by `-1` and `0`; Lua 5.4 semantics: `//` and `%` by
  zero are errors, `mininteger // -1` wraps, `mininteger % -1` is 0).
- Every recursion in the parser and compiler has a depth guard, including
  suffix chains (`a.b.c…`, `a[b][c]…`, `f()()…`, `a:b():c()…`),
  right-associative operators (`..`, `^`), unary chains, nested
  expressions, tables, functions and blocks; and dropping a deep syntax
  tree does not recurse unbounded.
- The library crate denies panicking constructs (`unwrap`, `expect`,
  `panic!`, `unreachable!`, `todo!`, `unimplemented!`, slice indexing),
  for example with `#![deny(clippy::unwrap_used, clippy::expect_used,
  clippy::panic, clippy::unreachable, clippy::todo,
  clippy::unimplemented, clippy::indexing_slicing)]` outside tests. A site
  that cannot fire may stay with `#[expect(clippy::…, reason = "…")]`
  saying why. Integer division, remainder and shifts on values a script
  controls use checked or wrapping forms; capacity computations use checked
  arithmetic (a `Vec::with_capacity` overflow panics too). `RefCell`-style
  borrows that a script's re-entry could conflict on are audited.

### D4. Lua 5.1 behaviour, opt-in

With the `Lua51` setting (as behaviour PUC Lua 5.1.5 shows):

- Numbers convert to strings with `%.14g` (`1/3` → `0.33333333333333`,
  `1e15` → `1e+15`, `2^53` → `9.007199254741e+15`, `-0.0` → `-0`,
  integral floats without `.0`, `inf`, `-inf`, `nan`), in `tostring`,
  concatenation and `print`.
- Integer `%` and `//` by zero follow float arithmetic (`1 % 0` → `nan`)
  instead of erroring.
- `string.format` follows 5.1: `%d`/`%i`/`%x`/`%o`/`%c`/`%u` truncate
  floats, flags `-+ #0`, width and precision at most two digits each
  (longer is the error "invalid format (width or precision too long)"),
  `%q` as 5.1 quotes.

Always (these are in every Lua version):

- A string metatable with `__index = string`, so `s:lower()` works.
- `string.byte`, `char`, `find`, `format`, `gmatch`, `gsub`, `len`,
  `lower`, `match`, `rep` (with 5.2+'s optional separator), `reverse`,
  `sub`, `upper`; Lua patterns with classes `%a %c %d %l %p %s %u %w %x`
  (and 5.1's `%z`), sets and ranges, anchors, `-`, `*`, `+`, `?`,
  captures including position captures `()`, back-references `%1`–`%9`,
  `%b`, `%f`; `gsub` with string (`%0`–`%9`, `%%`), table and function
  replacements and a count.
- `xpcall(f, handler, ...)`; `error(message, level)` adding
  `"<chunk>:<line>: "` for levels 1 and 2 as PUC Lua does (piccolo keeps
  `opcode_line_numbers`).
- `math.random`/`randomseed` without entropy: a seed set through the API
  (for example `Lua::set_random_seed(u64)`), deterministic for a seed;
  `rand` without default features, so a wasm build has no imports.

Not wanted here (the game adds them on top of the API): StepMania's `cmd`
syntax, 5.1 aliases such as `unpack`, `table.getn`, `math.mod`,
`loadstring`, the StepMania host API.

### D5. Tests and harnesses

- `tests/limits.rs`: every case of the list below, run with
  `Limits { memory: Some(32 << 20), string_len: Some(1 << 20) }`, 1,000
  fuel a step, the host stopping after 2 s of steps, under a counting
  global allocator. Each case asserts its expected outcome and records:
  steps, fuel used, peak real heap above the baseline (taken once the
  source is in memory), the longest single
  `Lua::enter` that ran one step (gc-arena's collection included), and the
  largest ratio of step time to fuel used. Run in release for the time
  gates (`cargo test --release --test limits`).
- `tests/lua51/`: scripts and their expected output, generated with PUC Lua
  5.1.5 built from `https://www.lua.org/ftp/lua-5.1.5.tar.gz` (sha256
  `2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333`), run
  with the `Lua51` setting. A script `tests/lua51/regen.sh` rebuilds the
  expected outputs from a local PUC build; the outputs are committed so CI
  needs no PUC Lua. Coverage: number formatting and `tonumber` (hex,
  exponents, whitespace, invalid input), coercions (`10 .. 20`,
  `"10" + 1`), every string function with its edge cases (negative and
  out-of-range indices, empty strings, every pattern feature above, `gsub`
  replacements and counts, every `format` directive and flag), `xpcall`,
  `error` levels and non-string error values, `select('#')` and negative
  `select`, `table.concat`/`insert`/`remove`/`sort`, `math` functions.
- A wasm harness (a `cdylib` built for `wasm32-unknown-unknown` with
  `panic = "abort"`, release) that runs a source string under the same
  limits and returns the outcome, and a Node script running every limits
  case through it, recording the growth of `memory.buffer.byteLength`.
- Two fuzz targets (cargo-fuzz): arbitrary bytes as a chunk, and programs
  generated from a Lua grammar (`arbitrary`-driven, every statement and
  expression form, the whole standard library and string methods, with
  the limit cases' sources as seeds). Each runs a chunk with the limits
  above and a budget of 10⁶ fuel, with debug assertions and overflow checks
  on. A crash, an overflow, a run past the budget or a heap past the cap
  is a finding, fixed and added to `tests/limits.rs` or the unit tests.

### D6. `PATCHES.md`

At the fork's root: each patch (what, why, the test that covers it), the
`Limits` API, the cost model, the `Lua51` differences, the known
remaining differences from PUC Lua, and the lints denied.

## The limit cases

Expected outcomes: **host stop** (the executor was still returning every
step when the host stopped it), **error** (a Lua error), **limit** (the
uncatchable limit error), **compile error**, **ok**. "Either" means any of
the listed, never a crash or a step past the gates.

| Id | Source (sketch) | Expected |
|---|---|---|
| C1 | `while true do end` | host stop |
| C2 | `while true do pcall(function() while true do end end) end` | host stop |
| C3 | `local co = coroutine.create(function() while true do end end) coroutine.resume(co)` | host stop |
| C4 | `local mt = {} mt.__tostring = function(t) return tostring(setmetatable({}, mt)) end print(tostring(setmetatable({}, mt)))` | host stop, error or limit |
| C5 | `local function f() return 1 + f() end f()` | error (stack overflow) or limit |
| C6 | `local function f(n) return pcall(f, n + 1) end f(1)` | error or limit |
| C7 | `local function f() coroutine.resume(coroutine.create(f)) end f()` | error or limit |
| C8 | `__index` and `__concat` functions that recurse into themselves | error or limit |
| C9 | `table.sort` of 10⁶ numbers; of 100 with `function() return true end`; with a comparator that errors | ok or host stop; ok or error; error (the comparator's) |
| C10 | `#t` and `next` loops over a 10⁶-entry table, repeated | host stop or ok |
| C11 | `string.find(("a"):rep(30), ("a*"):rep(30) .. "b")`, and the same with `.-` and nested captures | host stop or error (pattern too complex) |
| M1 | `local s = "x" for i = 1, 40 do s = s .. s end` | limit |
| M2 | `local t = {} for i = 1, 1e9 do t[i] = i end` | limit |
| M3 | `local t = {} for i = 1, 1e7 do t[i .. ""] = i end` | limit |
| M4 | `string.rep("x", 1.5e9)`; `("x"):rep(1e9, ",")`; `("x"):rep(-1)` | limit; limit; ok (empty) |
| M5 | `table.concat` of 64 strings of 512 KiB | limit |
| M6 | `table.unpack({}, 1, 1e8)`; `string.byte(s, 1, -1)` on 1 MiB; `string.char(table.unpack(t))` with 10⁶ entries | limit or error ("too many results"); same; ok, limit or error |
| M7 | `local s = ("x"):rep(2^20) s:gsub(".", s)` | limit |
| M8 | `local s = ("x"):rep(2^20) for i = 1, 1e6 do local u = s:upper() end` | host stop (fuel charged per byte) |
| M9 | `while true do pcall(function() local t = {} while true do t[#t + 1] = ("x"):rep(2^19) end end) end` | limit (not swallowed by `pcall`) |
| M10 | `string.format("%099999d", 1)`; `("%5.2s"):format(("x"):rep(2^20))` | error; ok |
| P1 | `a.b.b…`, `a[a][a]…`, `f()()…`, `a:b():b()…`, each 100,000 deep | compile error |
| P2 | `1+1+…` (left), `"a".."a"..…` and `2^2^…` (right), `- - -…`, `not not …`, `((…))`, `{{…}}`, nested `function … end`, nested `do … end`, nested `if`, each 100,000 deep (functions 5,000) | ok or compile error |
| P3 | a 16 MiB chunk of comments; a 1 MiB string literal; `[==[` long brackets unterminated; a table constructor of 2·10⁶ entries | ok, compile error or limit |
| I1 | `math.mininteger % -1`, `math.mininteger // -1`, the same with literals (constant folding), `1 % 0`, `1 // 0`, `math.maxinteger + 1` | ok (0), ok (wraps), same, error, error, ok (wraps); with `Lua51`: `1 % 0` → `nan` |
| I2 | `("x"):sub(math.mininteger, math.maxinteger)`, `string.format("%d", 2^63)`, `math.floor(0/0)`, `math.floor(1e308)`, `string.char(256)`, `select(math.mininteger, 1)`, `table.insert({}, math.maxinteger, 1)`, `table.unpack({}, math.maxinteger - 1, math.maxinteger)`, `tonumber("0x" .. ("f"):rep(100))`, `tonumber("1e99999")` | each ok or error, as PUC Lua 5.4 (5.1 with `Lua51` where it differs) |
| I3 | `for i = math.maxinteger - 1, math.maxinteger do end`; `for i = 1, 10, 0 do end`; float loops near 2⁵³ | ok (terminates); error; ok |
| E1 | `error(setmetatable({}, {__tostring = function() while true do end end}))`, the host then formatting the error | error, formatted without running Lua |
| E2 | `error()`, `error(nil)`, `error({})`, errors raised inside `xpcall` handlers, 10,000 nested `pcall`s | error or limit |

The agent adds any case its fuzzing or reading finds, with its outcome.

## Definition of done

All of these hold, and the report shows the evidence:

1. **Builds.** `cargo build` and `cargo test` pass natively on latest
   stable; the library and the wasm harness build for
   `wasm32-unknown-unknown`; the harness's release `.wasm` has no imports
   (`wasm-objdump -x -j Import` lists none). No new `unsafe`; no new
   runtime dependencies.
2. **Existing behaviour kept.** piccolo's own tests (`tests/`) pass
   unchanged, with the `Lua51` setting off; any test changed is listed with
   the reason.
3. **Lints.** `cargo clippy --lib` with the denied lints of D3 passes;
   every `#[expect]` has a reason.
4. **Limits, native.** Every case of the table ends in its expected outcome
   in `cargo test --release --test limits`, and for every case:
   - peak real heap above the baseline ≤ 32 MiB + 2 MiB;
   - the longest single step (one `Lua::enter` with one `Executor::step`
     of 1,000 fuel, gc-arena's collection included) ≤ 10 ms, and for every
     `Executor::step` call over 0.5 ms (collection excluded) the time per
     fuel unit used ≤ 1 µs (on an Apple M-series or comparable desktop;
     the report names the CPU);
   - no case is stopped by the watchdog.
5. **Limits, wasm.** The same cases through the wasm harness under Node:
   no trap, the expected outcome, memory growth ≤ 32 MiB + 8 MiB; the
   recursion guards' deepest accepted inputs run with the default stack.
   Step times are reported (no gate).
6. **Lua 5.1.** Every script in `tests/lua51/` matches PUC Lua 5.1.5's
   output byte for byte with the `Lua51` setting, apart from differences
   listed in `PATCHES.md` with a reason (error message wording may differ;
   results may not).
7. **Fuzzing.** Each fuzz target reaches at least 10 million executions
   with no open finding; the report gives the command lines, the execution
   counts and the findings fixed.
8. **Documented.** `PATCHES.md` as in D6.

## Report

When done (or blocked), report: the commits (hash and summary), the
`Limits` API as built, the results table of the limit cases (outcome,
steps, fuel, peak heap, longest step, worst time per fuel; native and
wasm), the fuzzing runs, the `Lua51` differences left, and anything that
needed a decision (a gc-arena change, a gate that could not be met and
why).

## Second round (2026-10-11)

The first round met every gate above. Integrating it into the game
(`lua/`, `ddi-lua`: Lua 5.1 mode, 32 MiB and 1 MiB limits, 1,000 fuel a
step) and an independent review found ways past the guarantees that the
limit cases did not cover. Measured on an Apple M1 Max, release builds.
Fix these on the same branch, one commit each with its limit case, then
publish.

| Id | Source (sketch) | Now | Required |
|---|---|---|---|
| R1 | `local t = setmetatable({}, {}) getmetatable(t).__call = t return t()` | every hop `push_front`s the callee into the arguments (`meta_ops::call`), shifting them all uncharged: steps of ~1,000 fuel take 6 ms after 2 s, 32 ms after 40 s; from `t(table.unpack(a))` with 250,000 values, 19 ms native, 18 ms wasm | the shift charged (`cost::VALUES`) or the chain bounded; host stop or error, every step within the gates |
| R2 | 65,536 strings built at run time whose interner hash collides (see below) | the interner hashes with `AHasher::default()`, fixed keys known to anyone with the source: one step of 4,785 fuel took 37 ms (the interner's rehash), 55 ms in a loop; the same count of ordinary strings peaks at 0.9 ms | hashing keyed by a seed the host sets per instance (`Lua::set_hash_seed(u64)`, for the interner and tables alike, as `set_random_seed`), so collisions cannot be computed offline; probes charged where a step can make many |
| R3 | `local keep = {} for i = 1, 60000 do keep[i] = {i} end while true do local s = ("x"):rep(2^19) end` | about 15 MiB live, the rest garbage: the uncatchable limit fires at 29.8 MiB because uncollected garbage counts and nothing collects before refusing | a script whose live data stays under half the memory limit never reaches the limit, whatever garbage it makes (a full collection before an allocation is refused, or between steps once near the limit); host stop |
| R4 | `string.find(("a"):rep(30), ("a*"):rep(30) .. "b")` (C11) | 10^6 matcher steps per attempt in one step: 6.7 ms native at `opt-level = 3`, 9.9 ms at `opt-level = "z"` (the game's wasm profile), 11.1 ms in wasm | the cap lowered so one attempt fits 2 ms in wasm (10^5 is ample for skins), or the matcher suspends inside an attempt |
| R5 | `string.format('%q', ('\0'):rep(2^20))`, default mode | output checked and charged after writing: 14.5 ms native, 36 ms wasm, for 24 fuel | charged and checked before writing, as Lua51 mode does (1.3 ms) |
| R6 | `load(('x=' .. ('9'):rep(20) .. ' '):rep(40000))` | 16.5 ms native, 17.5 ms wasm, ending in the uncatchable limit | `LOAD_MAX_WORK` set so a `load` fits the step gates; exceeding it a catchable error |
| R7 | (gates) | step times are thread CPU time; compile time is not gated (P3a takes 34 ms) | gate wall time too; report compile time per case |

R2's strings: brute-force integers `n` for which `"k<n>"` hashes, with the
interner's hasher, to a value whose low 16 bits are zero (46 s on 8
threads), and have the script build each with `"k" .. n` from a string of
the numbers (so the chunk itself holds no colliding constants):

```rust
use std::hash::Hasher;
fn h(s: &[u8]) -> u64 { let mut st = ahash::AHasher::default(); st.write(s); st.finish() }
// for n in 0.. { if h(format!("k{n}").as_bytes()) & 0xffff == 0 { keep n } }
```

With a host seed, the same search must find nothing better than chance;
the limit case then keeps strings colliding under the default seed and
checks they no longer collide under another.

Lower priority, document if not done: the ASan fuzz runs and their eight
`oom-*` files (they replay clean without ASan, at most 30 MB RSS);
`PATCHES.md`'s "about 0.25 µs per fuel for any work" and "about 5 ms" for
the matcher cap, both too optimistic as measured; runtime errors as
positioned strings (`attempt to index a nil value` with a line), which the
game's import report would show.

Done when the eight gates above still hold, R1–R7 are limit cases that
meet them (natively and in wasm), and the game's `ddi-lua` tests pass
against the new revision.

## After the second round (2026-10-11)

Checked on `c8a4b77`: piccolo's suite, clippy, the limit cases' outcomes
and CPU-time gates natively and in wasm, and the game's `ddi-lua` and
noteskin tests and corpus on it, all pass. The wall-time gates could not
be checked (the fuzzers kept the load average near 200). One gap left,
older than both rounds:

| Id | Source (sketch) | Now | Required |
|---|---|---|---|
| R8 | `local s = ('x'):rep(2^20) while true do local r = ('%5.2s'):format(s) end` (also `%.2s`, and `%5s` in the default mode) | with a width or precision, `%s` scans the whole argument for zero bytes (`split` to the first zero, and `contains(&0)` in the default mode) uncharged: about 25 fuel a call for about 0.75 ms on a loaded machine, 25–35 µs per fuel, steps of 60–100 ms; the same on `5db689c` | the scan charged (`cost::BYTES_PROCESSED`) or, in Lua51 mode, stopped at the precision; a limit case meeting the gates |

## Consolidated (2026-10-12)

The fork's maintainer reviewed and consolidated both rounds into one
commit, `75ab65a` on branch `hardening2` (not descended from `c8a4b77`;
among its changes, `table.concat` takes strings and numbers only, as in
Lua 5.1, rather than any value with metamethods). The workspace builds
on it unchanged, and checked on it:

- every native test of the workspace (`just test`), and a test that
  `print` and `table.concat` behave as Lua 5.1's;
- the 57 local skins on disk (StepMania 5.0.12's, OutFox's, Delta,
  Alpha-Arrows, Dake, Peter Lawrence's, the bundled ones) loaded and
  played at 60 and 144 Hz on `c8a4b77` and on `75ab65a`: the same elements
  and no warnings on both, load and frame fuel within 1 % (none of these
  skins calls `table.concat`);
- the fork's exported limit cases (125) through `ddi-lua`: every outcome
  as expected, the longest step 7.5 ms (the gate is 10);
- in headless Chrome and Firefox: full autoplays with Dito, plays with
  Delta, Alpha-Arrows and Dake, and the hostile test skins (one whose
  `NoteSkin.lua` never returns is stopped at import in 1.2 s; one whose
  update function loops stops in play and the built-in skin takes over);
  the wasm is 6 KB smaller.

R8 is still open on it: `('%5.2s'):format(s)` and `('%.2s'):format(s)`
on a 1 MiB string charge about 92 fuel per millisecond (other code about
70,000), steps up to 12 ms; the deadlines bound what it costs (a frame
stops at 50 ms, a load after its Lua's 5 s, in slices).

## Master (2026-10-13)

The maintainer revised the consolidated commit on the fork's `master`
(`40c4115`, again one commit, not descended from `75ab65a`): Lua 5.1
mode counts in floats as PUC 5.1 does, metamethod chains and `__index`
tables follow PUC's rules, more 5.1 library behaviour is tested byte for
byte, and the library loaders and `Context::set_global` return a
`LimitError` (refused only under a memory limit; `ddi-lua` loads its
globals before setting its limits and passes the error on). R8 is kept
as a known cost (`DELIBERATE(cost)` in the fork: the deadlines bound it).
Checked on it:

- every native test of the workspace (`just test`);
- the local corpus with each skin's Lua (`DDI_NOTESKINS_LUA=1`: StepMania
  5.1's, ITGmania's and the community mirror, 349 skins): the same
  outcome on `75ab65a` and `40c4115` for every skin (all load, the same
  actors and built-in elements), load fuel 0.3 % higher on average;
  played at 60 and 144 Hz, at most 49,953 fuel in a frame (49,882
  before);
- the fork's exported limit cases (126) through `ddi-lua`, natively in
  release and in wasm under Node: every outcome as expected once four
  more differ for Lua 5.1 (C6: `pcall` catches the overflow, where the
  memory limit stopped it at 16.5 MiB; I2h, I3a: 5.1 counts in floats, so
  `math.maxinteger - 1` is 2^63; I3b: a zero step runs no iteration;
  listed in `lua/tests/local.rs`), the longest step 7.2 ms natively, no
  trap in wasm and memory growth at most 32.4 MiB (38.4 before).
