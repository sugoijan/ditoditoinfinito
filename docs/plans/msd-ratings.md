# MSD chart ratings and a skill radar

The maintainer (2026-11-18): chart ratings of the game's own in place of the
simfiles' difficulty numbers, and a skill radar, both behind options Off by
default. Their own simulation-based rating (`teacher-v1`) is not ready
("too slow and too flat a scale"), so until it is: "Maybe we can use MSD
for the feature and replace it with a new system when it's ready."

MSD is Etterna's chart difficulty (its calculator, MinaCalc, MIT): an
overall rating and seven skillsets for keyboard play, at a music rate and
a score goal; the listing is rate 1.0, goal 93%.

## Stage A: the calculator (built, 2026-11-18)

- `crates/msd/` (`ddi-msd`): MinaCalc version 527 at etternagame/etterna
  `2d706147`, translated file for file for four keys; its README says what
  was translated, what was left out (five to seven keys, debug output, the
  XML parameters) and where it differs on purpose (a new state every
  rating). Pure, natively tested, built for wasm; `libm` (already in the
  tree) for `erf`, `erfc` and `pow`, so native and wasm agree to the bit;
  `fastsqrt` as upstream's ARM build computes it, in software.
- A chart's rows (`ddi_msd::chart::rows`): taps and hold and roll heads in
  judged time as bits; mines, lifts, fakes and keysounds a row without
  bits (the first row of any kind is the time origin, as in Etterna). Rated
  layouts: four lanes for one player, `ddi_msd::chart::four_keys` (of the
  built-in layouts dance-single, generated versions on it included; the
  charts the CPU plays). Every other chart keeps the simfile's number.
- The oracle: `tools/msd-oracle/build.sh` builds upstream's calculator by
  itself from the pinned commit (fetched into `target/msd-oracle/`) as a
  small program; `crates/msd/tests/oracle.rs` (opt-in) compares the port
  against it. Synthetic and bundled charts: 113, 109 identical to the bit,
  the largest difference 4e-6; a local corpus of 5,656 charts: 5,436
  identical, the rest within 2e-6. Tolerance 0.005; the oracle's numbers
  for the synthetic and bundled charts are committed
  (`crates/msd/tests/fixture.json`) and checked natively and under WASI.
- Cost: about 0.2 ms a chart natively (release, median; worst 4 ms), 0.3 ms
  debug, 0.4 ms in wasm.

## Stage B: the feature (planned)

Two options (Chart ratings: From the song / MSD; Skill radar: Off / On),
ratings for the bundled songs at build time and for imported and library
charts in the background, paused during play and cached by chart digest
and calculator version; the MSD number wherever a chart's difficulty
number shows; a radar of the seven skillsets on song select.
