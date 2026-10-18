# Rust tempo analysis and musical grids

Implemented 2026-10-13 as the musical-analysis portion of
[milestone 3](danoni-rust-plan.md). The embeddable Rust core now fits chart
lattices, derives audio pulses, selects musical grids, handles shared music
with independent chart phases, and validates source-bound cached decisions.
The CLI calls that same API. No Python, Node, FFmpeg or browser is required
for this stage. Source acquisition and runtime extraction remain separate work.

Two producers are deliberately distinct. An explicit bridge applies the Rust
selection policy to preserved legacy analysis and reproduces the accepted
choices. Fresh Rust spectral analysis is **experimental**: it produces different
musical aliases and phases on some tracks. A bounded timing result is not a
certificate of the music's BPM or downbeat. The accepted 93-song packs remain
unchanged; none of the new analysis results has been promoted.

## Library boundary and units

`tempo::prepare_tempo(&TempoRequest,&AssetSource,checkpoint)` returns a JSON
`ddi-danoni-tempo-result/1`. Like `prepare_work`, it receives immutable, hashed
assets through host-supplied namespaces. It does no filesystem/network access
or work-script execution. The checkpoint propagates cancellation during audio,
search and per-track work. The host schedules this synchronous CPU operation
outside the UI thread.

```text
TempoRequest:
  manifest: Binding
  media: AnalysisPcm[]                 default []
  legacyAnalysis: Binding | null       default null
  cached: Binding | null               default null

AnalysisPcm:
  sourceAudioSha256: lowercase SHA256
  pcm: Binding
  receipt: Binding
```

`Binding` is `{namespace,path,sha256}` as specified in the
[offline contract](danoni-rust-offline.md). The source manifest is the existing
normalized work document, with original source charts/music/references. The
core verifies source bindings and original lane/hold relationships before using
any estimates. The analysis limit is 16,384 distinct head timestamps per fitted
set; oversized sets reject rather than silently thinning notes. Chords count
once for fitting. Holds contribute their heads, while **both endpoints** are
checked during validation. This analysis limit does not change the chart
preparation limit of 1,000,000 endpoints.

Native requests supply one proved PCM16 WAV per source audio hash, grouped
independently of chart count. Its neutral `ddi-danoni-prepared-media/1` receipt
must bind the exact source manifest, original audio hash, PCM hash, sample rate,
channels and sample-frame count. The [audio stage](danoni-rust-audio.md) produces
these inputs; analysis does not realign audio or rewrite source note times.

Alternatively, `legacyAnalysis` binds one preserved
`ddi-tempo-analysis/experimental-1` document. It must match the source manifest,
source audio hashes and each track's complete chart IDs. Native PCM and legacy
analysis cannot be combined in one request. Fixed chart fits and stable-window
statistics are recomputed against current source heads; legacy pulse timestamps
remain explicitly supplied evidence, not new Rust audio analysis.

All source/head/pulse/phase times are **audio-relative seconds**. BPM is quarter
beats per minute; subdivisions are 1/2/3/4/6/8 per quarter. Residual fields ending
in `Ms` use milliseconds. PCM offsets count sample frames, not interleaved values.
The final musical grid accepts 60–360 BPM, defaults to a 9 ms endpoint budget,
and uses 48 SSC ticks per beat. Diagnostic window fitting permits a wider tempo
range than the final selected grid.

## Implemented algorithms

The versioned Rust policy ports the existing MIT
[chart/audio analysis](danoni-consolidation.md#retired-tools) (retired),
[automatic selection](danoni-consolidation.md#retired-tools) (retired) and
[shared-chart fitting](danoni-consolidation.md#retired-tools) (retired):

- Circular Fourier coherence and wrapped residuals; 60–360 BPM coarse search,
  fine peak refinement, binary/triplet aliases, and interpolated residual
  quantiles. A grid period cannot be shorter than three 60 Hz source frames.
- Robust pulse regression with missing-beat indices and outlier rejection;
  circular phase evidence when a global linear pulse model does not hold.
- Twenty-second chart windows and a dominant stable cluster. Unstable portions
  remain in the source and use fine ticks when necessary; no notes are removed.
- Authored BPM-range hints, chart/audio agreement and generic alias priors.
  Shared music gives charts equal votes, refines nearby tempos independently,
  requires a supporting majority, and retains independent chart phases. A
  uniform-phase safeguard avoids treating accidental alignments as support.
- Whole-chart lane collision handling and complete endpoint/speed-event
  validation before exposing any review. Musical grids change colouring and
  BPM metadata; the existing native-motion compensation preserves base scroll
  beneath the user's multiplier.

Fresh onsets use **`ddi-spectral-flux-dp/3`**, an independent implementation,
not a numerical reimplementation of librosa's beat tracker. It resamples to
22,050 Hz, computes channel spectra independently with a 1,024-sample Hann FFT
and 64-sample hop, groups power into 64 mel-spaced buckets, and forms positive
log-power spectral flux. Local energy normalization limits domination by a loud
section. Global normalized autocorrelation proposes pulse periods at generic
60/120/180 BPM priors. A cumulative dynamic path uses Gaussian local support
(width one thirty-second of the candidate beat period) and a log-interval
penalty with tightness 100. It ends at an observed local maximum and preserves
later quiet sections rather than discounting history. Robust fitting then
refines the observed timestamps. The onset/correlation/path approach follows
the established [dynamic-programming beat-tracking stages](https://librosa.org/doc/0.11.0/generated/librosa.beat.beat_track.html);
this is independently implemented Rust, with no imported librosa source or runtime.
Nearby refined estimates within a log-BPM distance of 0.005 share one vote;
the lower residual representative survives. Priors are multiple views of the
same recording, not independent measurements. No song names or pack-specific
constants enter these algorithms.

Native selection **`ddi-tempo-selection/2`** accounts for the half-hop resolution
of a coarse integer-lag pulse proposal when comparing nearby BPMs. This avoids
breaking an otherwise equally supported chart alias through audio period rounding.
Precisely regressed pulses keep the old distance rule. The existing generic
150 BPM prior resolves ties; it does not prove which beat unit the author used.
The explicit legacy bridge retains **`ddi-tempo-selection/1`** unchanged.

After selecting a native grid, **`ddi-quarter-onset-support/1`** evaluates every
quarter-phase alias at that chart's precise BPM, not the rounded audio proposal.
It samples the measured onset envelope across the chart span with a 7 ms Gaussian
window truncated at ±20 ms. An alias needs at least eight observed quarter
positions, mean support above 1.25 times the span's mean onset level, and at
least 10% contrast over the next alias. Only then does it change phase by a whole
subdivision. That relabels beat colouring without moving source note times.
Equal eighth pulses remain ambiguous. A single subdivision alias has no runner-up
contrast and also stays unverified. Weak support retains the provisional phase;
this phase evidence never identifies a bar downbeat or proves musical correctness.
The bounded native envelope has the declared 22,050 Hz / 64-sample-hop dimensions.

The native search uses exact integer frequency indices. Twelve old peak choices
changed through floating-point ties or the inclusive three-source-frame boundary
(the old NumPy search could evaluate an intended boundary just beyond it). This
is an intentional candidate-producer difference: all 1,044 fixed legacy fit
statistics agree, and native search's best coherence per subdivision is at least
as good. Identical candidate sets are not claimed.

## Result, fallback and cache

```text
manifest.json: ddi-danoni-tempo-result/1
  status: "estimated" | "exact-source-fallback"
  sourceManifestSha256: SHA256
  cacheKey: SHA256
  cacheIdentity: object
  producer: explicit native/legacy scope
  tracks: track evidence[]
  review: ddi-reviewed-musical-grid/experimental-1 | null
  validation: object | null
  fallback: {code,message} | null
  fallbackPolicy: string
  sourceNotesModified: false
```

Each track retains `sourceAudioSha256`, `chartIds`, `analysis` (candidates,
pulses, dominant grid and authored hints), `chartWindows`, optional
`nativeOnsets`, and shared-chart evidence. Native onset evidence records the
algorithm/rate/hop/FFT, estimates, envelope length and envelope hash; the large
envelope itself is available from `onset_estimates`, not embedded in the job
result. Fit confidence is evidence, not a musical correctness probability.

An estimated `review` contains `sourceManifestSha256`, algorithm/cache identity
and `charts: {chartId: MusicalGrid}` for **every source chart**. Each grid has
`bpm,beatZeroSeconds,subdivisionsPerBeat,maximumErrorMs` plus method, fit,
phase and consensus evidence. Native grids also retain `nativeQuarterPhase`:

```text
algorithm: "ddi-quarter-onset-support/1"
status: "audio-supported-quarter-alias" |
        "ambiguous-quarter-alias" | "insufficient-quarter-onset-support"
relativeContrast: number
baselineMeanOnset: number
alternatives: [{beatZeroSeconds,meanOnsetSupport,observedQuarters}, ...]
sourceStartSeconds,sourceEndSeconds: number
scope: string
```

These are support measurements, not probabilities. `phaseStatus` explicitly
reports an unresolved provisional alias when no phase change is supported.
Phase can differ between charts sharing music.
The existing `WorkRequest.reviews.grid` accepts a binding to this review unchanged.

`validation` records `sourceEndpoints,maximumNoteErrorMs,rhythmicEndpoints,
allLaneRelationshipsAndSpeedEventsChecked`. Insufficient pulse/lattice evidence,
a timing-budget violation or unrepresentable lane suppresses the **entire work's**
review and returns `exact-source-fallback`. The host then passes no musical-grid
review to `prepare_work`, preserving the exact source clock. It must not apply a
partial set of charts from the evidence. Malformed/hash-mismatched sources,
resource limits and cancellation are errors, not guessed successful estimates.

`estimated` means the grid passes these mechanical checks. Musical alias and
quarter/downbeat phase remain provisional, especially with sparse, variable or
triplet patterns. Native quarter-phase support can resolve an observed alias,
but retained ambiguity and unresolved beat units still require review. Saved native results can be inspected or explicitly reviewed before
replacing accepted musical grids.

`cacheIdentity` (`ddi-danoni-tempo-cache-key/1`) binds the exact manifest,
all verified source input hashes (including legacy reference bytes), PCM/receipt
bindings or legacy-analysis binding, policy `ddi-tempo-selection/2` for native
analysis (`/1` for the legacy bridge), native quarter-phase algorithm identity, lattice
`ddi-chart-lattice/1`, onset producer/version, and dependency identities
(RustFFT 6.4.1, Rubato 5.0.1, Symphonia 0.6.1). `cacheKey` hashes its serialized
JSON. Changing any algorithm parameter requires a new producer/policy version;
these identifiers name the fixed configurations above. Altering any identity
input invalidates reuse. A cached result is a host-supplied
hashed binding; cache lookup/storage/eviction is the host's responsibility, not
an implemented disk cache manager. A cached estimated review is source-bound
and all chart endpoints/events are revalidated before reuse. Cached fallback
results preserve the same explicit whole-work outcome.

## CLI and output

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- estimate-tempo /tmp/tempo-job.json /tmp/tempo-new
```

The external job is `ddi-danoni-tempo-job/1` with `roots` and `request` as above.
Roots follow the offline containment rules and are relative to the job when not
absolute. Output must be a fresh external directory with an existing parent:

```text
grid.json       only for "estimated"; pass its binding to prepare_work
manifest.json   full tempo result; final rename marks readiness
```

Cancellation/failure removes the writer's owned partial directory. Output retains
source bindings, hashes and algorithm evidence, but does not copy PCM/music or
source inputs into the tempo folder. It is an analysis result, not a portable song
or a library publication transaction. For cache replay, supply a binding to this
`manifest.json` in `request.cached`, with all original inputs still available.
Library callers receive the same result in memory without this filesystem writer.

## First native checkpoint (v2 onsets), 2026-10-13

Receipts, source comparisons and CLI runs are local under
`~/Music/DanOni/reviews/rust-tempo-20261013/`:

- Nine synthetic tests cover rounded lattices/aliases, robust missing beats,
  authored ranges, independent phases/drift, dominant portions, sparse/nonfinite
  inputs, cancellation, opposite-phase stereo clicks, source-bound cache reuse,
  source changes, invalid lanes and exact-source fallback.
- The explicit legacy-analysis bridge reproduces **126 accepted chart decisions**
  over 87 available old analysis files, including method/subdivision and phase
  within 0.1 ms modulo a quarter beat. Manual reviews are not implied by that gate.
- **1,044 fixed lattice fits** match legacy coherence/residual statistics, with
  no regression in best native peak coherence per subdivision.
- Fresh native v2 analysis runs over **89 works / 93 tracks / 144 charts** without
  errors or fallback. Every applied note/hold endpoint passes the 9 ms budget;
  maximum measured error is **8.999081455 ms**. This is timing safety, not musical
  parity. The run takes about 229 seconds on this machine in the optimized dev
  profile; it is not a desktop latency guarantee.
- Complete-song preparation consumes these saved native reviews for **all 93
  songs / 144 charts**. The real game parser checks **105,459 endpoints**,
  including hold tails, and **3,057 x1/x2/x3 velocity probes** in intervals away
  from quantized event boundaries. Ratings/layouts and the original base scroll
  remain unchanged; endpoint error stays below 9 ms. The render probes run near
  the receptor to avoid subtracting distant f32 positions.
- A fresh Kouhaku CLI job and explicit cached replay produce identical complete
  result documents for its six charts, rechecking 2,493 endpoints on reuse.
- Relative to accepted grids, **115 charts** have BPM within 0.5% of the same
  family, **10** choose half tempo, **2** double tempo, **12** two-thirds tempo,
  and **5** other choices. Quarter-phase differences remain, including roughly
  half-quarter aliases. This comparison is chart-weighted, not 144 independent
  audio recordings. Accepted BPMs themselves include provisional/manual choices.

For example EMY selects about 99.879 instead of the accepted 199.759 BPM,
Brilliant Blue selects 220 instead of 110, and NEEDS selects about 168.064 instead
of 252.5. The octave/triplet ambiguity changes colouring and metadata even when
notes stay within budget. These examples identify review cases, not special
parameters. Keep accepted exports until a generic alias/phase policy is evaluated
against both synthetic ambiguous cases and this recorded corpus comparison.

Run the preserved-statistics/decision gates read-only with:

```sh
DDI_DANONI_TEMPO_GENERATION="$HOME/Music/DanOni/reviews/rust-packaging-20261013/published-trial" \
DDI_DANONI_EXPORTS="$HOME/Music/DanOni/exports" \
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test tempo_corpus -- --nocapture
```

Add `DDI_DANONI_TEMPO_MEDIA` pointing to the native codec corpus and
`DDI_DANONI_TEMPO_OUTPUT` to a **fresh** external directory for new native analysis.
For complete-song consumption of saved results, set `DDI_DANONI_TEMPO_RESULTS`
to that result directory. These are development gates; the app uses library
requests and supplies its own cache/storage. Milestone 3 remains open for native
musical-quality review; the Rust algorithms and mechanical gates are implemented.


## Alias and phase refinement, 2026-10-13

The subsequent trial combines v3 cumulative onsets, native selection policy v2,
and quarter-onset support v1. All results and intermediate tracker-only results
remain outside Git under `~/Music/DanOni/reviews/rust-tempo-v3-20261013/`.
Accepted packs are unchanged. Six additional synthetic cases cover a quiet late
section, strong offbeat subdivisions, integer-lag alias ties, strong measured
quarters, equally strong eighth ambiguity/timing invariance, and native media/
cache-version bindings. The last case rejects prior algorithm identities and
incorrect sample-frame receipts. Existing compatibility decisions retain their
original producer and cache identity.


The full refined run reports **88 estimated works / one exact-source fallback**,
covering the same **93 songs / 144 charts** with no errors. Of the 143 estimated
charts, **117** match the accepted BPM family within 0.5% (previously 115),
**8** choose half tempo, **2** double tempo, **12** two-thirds tempo, and **4**
other values. EMY and Regret Of The Enemy return to their accepted ~200 BPM
family; no previously matching BPM family moves away. The changing/weak fit in
Zankokuka returns an explicit `timing-budget-exceeded` fallback rather than
promoting an unsafe grid. That fallback accounts for the remaining chart.
These are comparisons with the accepted choices, not new musical ground truth.
The tracker-only v3 trial has 114 matching charts: the period-resolution policy
is evaluated separately rather than attributing the improvement to the tracker.

Quarter evidence reports **17 audio-supported aliases / 16 ambiguous / 110
insufficient** among the estimated charts. Support alone is not a claim that a
phase changed or that the selected beat unit is correct; HISTORY, for example,
still has a different BPM despite quarter support at that selected period.
The large insufficient set makes the remaining uncertainty visible instead of
calling every closest pulse an observed quarter phase.

Complete preparation with these reviews and the explicit fallback passes the
real game parser for **all 105,459 endpoints** and **3,057 x1/x2/x3 velocity
checks**. Maximum endpoint error is **8.998218203 ms**. The legacy bridge still
reproduces all 126 accepted decisions. Fresh CLI/cache replay equality and older
native cache rejection are separately checked. Native/pure tests, Clippy,
formatting and `just check` (including REUSE) pass.

The remaining review cases include NEEDS's binary/triplet beat unit, Cold's
half-tempo choice, the unstable audio/phase fits in Felys and HISTORY, and sparse
quarter support. The next quality work should compare local stable audio periods
and report alternative beat units, rather than force equality to legacy guesses
or insert work-specific preferences. Source extraction/supervision still needs
its Rust port; these musical limits do not alter the exact source contracts.
