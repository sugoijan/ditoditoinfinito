# ddi-msd: MSD for four keys

MSD is the chart difficulty Etterna computes with its calculator,
MinaCalc: an overall rating and seven skillsets (Stream, Jumpstream,
Handstream, Stamina, JackSpeed, Chordjack, Technical) for keyboard play, at
a music rate and a score goal. This crate is a translation of that
calculator to Rust, for four keys, used by the game as an optional chart
rating until a rating of its own replaces it
(`docs/plans/msd-ratings.md`).

## What was translated, from where

- Upstream: <https://github.com/etternagame/etterna>, `src/Etterna/MinaCalc/`
  at commit `2d706147cc4bcf95a8ceb24dc4f4129be207f803` (the `master`
  branch on 2026-11-18), calculator version **527**
  (`mina_calc_version`, [`CALC_VERSION`]). MIT licensed (`LICENSE` at that
  commit: "Copyright (c) 2016-2023 Etterna"); the calculator's files carry
  no headers of their own. Each translated file here keeps that copyright
  and the MIT notice in its header and names the upstream file(s) it comes
  from; `REUSE.toml` annotates them and `credits/THIRD_PARTY.toml` ships
  the notice on the Credits screen.
- File for file: `MinaCalc.h`/`.cpp` → `minacalc.rs`, `MinaCalcHelpers.h`,
  `PatternModHelpers.h`, `SequencingHelpers.h`, `CalcWindow.h`,
  `SequencedBaseDiffCalc.h`, `UlbuAcolytes.h`, `Ulbu.h` with the parts of
  `UlbuBase.h` it inherits → `ulbu.rs`, and `Agnostic/` and `Dependent/`
  module for module (`agnostic/`, `dependent/`; `Roll.h`/`RollJS.h` and
  `OHT.h`/`VOHT.h`, two copies upstream, share one body here with their own
  parameters). `NoteDataStructures.h`'s input row and enumerations →
  `note_data.rs`.
- The arithmetic is upstream's: `f32` where it is `float`, `f64` where it
  is `double`, in the same order; C++'s `std::min`, `std::max` and
  `std::clamp` as written (their NaN answers differ from Rust's); the
  `fastpow` bit trick as is; `erf`, `erfc`, `pow`, `powf` and `logf` from
  the `libm` crate (rust-lang's port of musl, already in the game's tree)
  so native and wasm give the same bits.
- `fastsqrt` is `x * _mm_rsqrt_ss(x)`, the processor's approximate
  reciprocal square root, whose last bits differ between processors. This
  crate computes it as upstream's build for ARM does through sse2neon
  (`FRSQRTE`, then one `FRSQRTS` Newton step), in software from the Arm
  Architecture Reference Manual's definition, on every target; checked bit
  for bit against that build on 329,740 inputs.

## Left out

- Five, six and seven keys (`UlbuFiveKey.h`, `UlbuSixKey.h`,
  `UlbuSevenKey.h`, the generic `Bazoinkazoink` with `GenericStream`,
  `GenericChordstream`, `GenericBracketing`, `oversimplified_jacks`). They
  share the driver and most helpers; adding them means translating those
  drivers and mods (about 20 KB of C++), generalizing the four-key masks
  in `ulbu_acolytes.rs` and the four-key-only sequencers, and the
  `keycount != 4` scaling in `CalcMain`: a round of its own, not nearly
  free.
- Values only upstream's debug graphs read: MSBase, JackBase, CJBase
  (`ceejay`), RMABase, the debug vectors, the per-hand moving window CVs.
- The XML parameter loading (`load_calc_params_from_disk`): only debug and
  development runs read parameters from disk; the defaults are the
  calculator.
- The PHP extension and the all-rates cache loop (`MinaSDCalc` for 0.7x
  to 2.0x): [`msd`] rates at one rate.

## Where this differs from upstream, on purpose

- Every rating starts from a new calculator's state. Upstream reuses one
  `Calc` a thread, and a few sequencers (FlamJam's, the two TheThing
  sequencers) are never reset between calls, so in rare charts (one that
  ends inside a flam) upstream's cached numbers can depend on which chart
  or rate its thread rated before. The oracle (`tools/msd-oracle/`) also
  makes a new `Calc` a chart.
- Inputs upstream would crash on (a row with no notes, a time before the
  first interval) are rated all zero, as upstream's skipped files are.
- `Chaos.h` calls `abs` unqualified on a float: libc++ and MSVC resolve it
  to the float overload, which this follows; a libstdc++ build may resolve
  it to `int abs` and differ.

## Validation

`tools/msd-oracle/build.sh` fetches the pinned commit's calculator files
and builds them, unchanged but for one debug-only call guarded, as a small
program (`oracle.cpp`), with `-ffp-contract=off` so clang does not fuse
multiplies and adds on arm64 (Rust never does, and neither does an x86_64
build without FMA). `tests/oracle.rs` (opt-in, `DDI_MSD_ORACLE`) compares:

- 82 synthetic charts made by `tests/common/` (streams, jumpstream,
  handstream, quads, stamina, jacks, long jacks, chordjacks, technical
  sections: rolls, one hand trills, runningmen, jumptrills, flams, mixed
  snaps, minijacks; edge cases; other rates, goals and the score path) and
  the bundled songs' 31 four-lane charts: 109 of 113 identical to the bit,
  the largest difference 4e-6 (Stamina's `powf` and, on the score path,
  the grindscaler's `logf`: Apple's libm against `libm` in the last bit).
- Opt-in, a local corpus (`DDI_MSD_CORPUS`, aggregates only): 5,656 charts,
  5,436 identical to the bit, the rest within 2e-6 (Stamina).

The tolerance is 0.005 a skillset; nothing comes near it. The oracle's
numbers for the synthetic and bundled charts are committed in
`tests/fixture.json`, which `tests/fixture.rs` checks without the oracle,
natively and for wasm (`--target wasm32-wasip1`, the same bits).

Upstream's own builds disagree more than the port and its oracle do: an
x86_64 build of the same commit (through Rosetta, its `rsqrtss`) differs
from the ARM build by more than 0.005 on 2.2% of the corpus, by up to 0.18
(Stamina).

## Cost

About 0.2 ms a chart natively (release, median over the corpus; worst
4 ms), 0.3 ms in a debug build, 0.4 ms in wasm (Node's V8; worst 3 ms on
the synthetic charts). `tests/cost.rs` measures it.

[`CALC_VERSION`]: src/lib.rs
[`msd`]: src/minacalc.rs
