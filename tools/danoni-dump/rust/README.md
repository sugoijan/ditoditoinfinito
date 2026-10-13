# Rust DanOni export core

The first slice of the [Rust migration plan](../../../docs/research/danoni-rust-plan.md).
`ddi-danoni-export` is an I/O-free library with a JSON CLI over the same
`compile_chart` function. It prepares an SSC **chart block**, not yet a complete
song/pack, and does not fetch URLs, decode audio, estimate BPM, run Ruffle or write
to the game library. Existing Python/Node commands remain the complete workflow.
Code is MIT under the repository REUSE annotation; all third-party inputs/results
stay outside Git. The standalone workspace keeps native tooling out of the web build.

## Commands

Run from the research checkout with Rust 1.99 or later:

```sh
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust
cargo clippy --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust --all-targets -- -D warnings
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust -- compile-chart < /tmp/request.json > /tmp/result.json
DDI_DANONI_EXPORTS="$HOME/Music/DanOni/exports" cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust --test local_corpus -- --nocapture
```

The local differential test reads accepted `reports/*/manifest.json`, original
source charts/merged metadata preserved in derived references, reviewed policy
inputs and accepted SSC files. It never downloads or runs work scripts, writes
corpus files or changes exports. No corpus means the test reports a skip explicitly.

## API and request

Call `compile_chart(&Chart, &Metadata, &PreparationOptions)` directly. All source
arguments are borrowed. The caller supplies already merged work/reference/chart
metadata and validated, hash-bound review decisions. The core does not certify
unverified caller-supplied motion/grid/rating evidence. Future native orchestration
will perform those checks before calling it. Unknown source chart/lane fields are
preserved in flattened maps and not evaluated.

CLI stdin is at most 16 MiB and contains exactly one request:

```json
{
  "schema": "ddi-danoni-compile-request/1",
  "chart": {
    "id": "normal",
    "mode": "5",
    "lanes": [
      {"tapsSeconds": [1], "holdsSeconds": []},
      {"tapsSeconds": [], "holdsSeconds": [[2, 3]]},
      {"tapsSeconds": [], "holdsSeconds": []},
      {"tapsSeconds": [], "holdsSeconds": []},
      {"tapsSeconds": [4], "holdsSeconds": []}
    ],
    "difficultyStars": 4,
    "speed": [{"seconds": 1, "multiplier": 1.5}]
  },
  "metadata": {},
  "options": {
    "grid": {"bpm": 120, "beatZeroSeconds": 0, "subdivisionsPerBeat": 2, "maximumErrorMs": 9},
    "nativeMotion": {"baseArrowHeightsPerSecond": 2.8},
    "scrollFactor": 1,
    "tierPolicy": "stars",
    "tierBounds": [1, 3, 5, 7],
    "rating": {"kind": "source"},
    "preserveLabel": false
  }
}
```

Only `schema` and `chart` are required; metadata defaults empty, and each option
has the shown default except `grid`/`nativeMotion`, which default null. With no
grid, exact source frames become ticks at 75 BPM. With a grid, finite
60–360 BPM, phase within ±600 s and subdivisions 1/2/3/4/6/8 are accepted; the
budget must be positive and below 10 ms (default 9 ms). Times are audio-relative
seconds; custom note times can be quantized with a grid, while speed event times
retain the existing exact-source-frame requirement. Lane order is the normalized
built-in order documented in the preparation contract, not guessed from lane names.

`rating:{"kind":"reviewed","stars":N}` supplies an already validated source
rating; null explicitly selects unknown. Source rating extraction honors numeric
chart-over-metadata precedence and pure ★ strings, never arbitrary difficulty
names. Unknown uses Edit/0. Bounds are inclusive and strictly increasing.
`tierPolicy:"edit"` selects Edit without discarding a known numeric meter.
Chart `metadata.difficultyLabel` becomes CHARTNAME for unrated charts or when
`preserveLabel` is true. Unknown modes (including 9i/9t) reject deserialization.

## Result and units

Success stdout is `ddi-danoni-prepared-chart/1`:

- `id`, `mode`, `difficultyStars` (integer or null), `difficultyClassification`.
- `timingBpm`, `offsetSeconds` (SSC sign convention), `paddingTicks` and
  `sscBaseRatio` (source arrow heights/s × factor × 60 / timing BPM).
- `lanes[]`: `taps` and paired `holds`, signed **unpadded** ticks, 48 per beat,
  plus `collisionSafeguard` when the whole lane uses fine-tick fallback.
- `endpoints[]`: lane/index, `kind` (`tap`, `hold-head`, `hold-tail`),
  `sourceSeconds`, `exportSeconds`, `shiftMs`, signed unpadded `tick`, and
  `method` (`exact-source-frame`, `rhythmic-subdivision`, `fine-tick-fallback`,
  `lane-collision-fine-fallback`). Both hold endpoints are included.
- `scrollEvents[]`: source/export seconds, shift ms, unpadded tick and original
  multiplier; effective SSC ratios also include `sscBaseRatio`.
- `maximumNoteErrorMs`, `unrepresentedEffects` (nonempty colors/boost/stops/effects
  counts) and `ssc` (one complete NOTEDATA block; no music/banner/preview headers).

Native motion adds a baseline SCROLLS segment and preserves the user's multiplier.
Musical pre-roll is padded by whole beats; positive phase with native motion may
add whole measures to make the baseline active before audio zero. Source seconds
are not rewritten. Source/policy evidence remains the caller's responsibility.

Failures exit nonzero, emit `{code,message}` JSON on stderr and produce no result.
Request/options reject unknown fields to catch configuration typos. Source maps
allow unknown factual fields. Finite-number and relationship checks apply equally
to direct library calls and JSON calls.

Intentional tightening over the old helpers: reject >999 raw stars, nonfinite or
>±7200 s source times, more than 1,000,000 note endpoints, >100,000 speed events,
>20,000 emitted measures, malformed effect arrays and scroll multiplication
that overflows. These do not change the accepted corpus. Python's ties-to-even
rounding is preserved. SSC uses shortest round-trip Rust decimal formatting;
whitespace/numeric spelling and archive bytes are not promised identical.

Not yet ported: song grouping/headers, review-file binding, previews, artwork,
media processing, BPM inference, rebuild transactions, fetching and execution.
