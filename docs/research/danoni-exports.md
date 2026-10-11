# Regenerating the local DanOni exports

Updated: 2026-10-11. `ruffle/rebuild_corpus.py` now builds the
currently validated subset entirely from saved evidence, checks it with the
game's actual parsers, and promotes it with archival and rollback journals.
[Native family adapters](danoni-native-adapters.md) now cover eight further works.
The [CW bridge and default BPM fitting](danoni-default-tempo.md) are now implemented. The registration-gated original Straight
Soul was deliberately removed from the active list; its HTML5 port remains.
All third-party inputs and generated packs stay outside Git.

## Current generation

The [gen3 native admission](danoni-gen3-admission.md) adds eleven source works
and 21 proved charts, including Cynthia's two seven-key charts: **44 songs /
72 charts, 52,627 taps and 1,906 holds**. Every song has a banner. The registry
has 40 required and 96 pending source entries; one extra タイヨウノハナ selector
remains explicitly pending within an admitted work. The previous 33 songs retain
their accepted playable files.

## Previous paired-source admission

The [paired-source admission](danoni-paired-ports.md) added Saigetsu and the
レク chart of Earthquake Super Shock: **33 songs / 51 charts, 40,977 taps and
1,230 holds**. Its registry had 29 required and 107 pending entries. The prior
31 songs remained byte-identical.

## Previous v2 admission

The [first v2 admission batch](danoni-corpus-v2-admission.md) expands the accepted
exports to **31 songs / 49 charts, 39,021 taps and 1,176 holds**. The registry now
has 136 entries: 27 required and 109 explicit pending sources. All 31 songs have
banners. The previous generation below is retained as the comparison baseline.

## Previous accepted generation

The local registry is `~/Music/DanOni/export-registry.json`. Its 23 entries
include 15 required, successful works and eight explicit pending entries:
incomplete/unsupported CW captures or exploratory probes. Original Straight
Soul is excluded; its acquisition and exclusion provenance remain local.
Pending entries are inventory, not playable exports. The generation contains **19 songs, 23 charts, 12,896 taps and 332 holds**.

| Work | Songs / charts | Output and validation |
| --- | --- | --- |
| Kouhaku | 5 / 6 | SSC/Ogg, accepted chart data/audio unchanged; new banner header; reviewed musical grid, display BPM, pickup previews and native scroll; maximum note movement 8.893 ms |
| Higan Normal | 1 / 1 | SSC/Ogg, default bounded musical fitting; 12 author speed events and its own measured native baseline; 342 expanded source colour events retained only in evidence |
| Jam with Pancake | 1 / 1 | CW-derived SSC/Ogg with title banner, 7-key; 1,160 taps, 140 holds |
| Straight Soul HTML5 | 1 / 2 | CW-derived SSC/Ogg with title banner, 5/7-key; 2,659 taps, 18 holds |
| カナリア | 1 / 1 | CW-derived SSC/Ogg with title banner, 7i; 825 taps, 37 holds |
| Dancing at Dusk | 1 / 1 | CW-derived SSC/Ogg with title banner, 5-key; 546 taps, 51 holds |
| ふしぎなひかり | 1 / 2 | CW-derived SSC/Ogg with title banner, 9A/9B only; 237 taps, four holds; unsupported 9i omitted |
| FTHosyu / party 4u / felys | 3 / 3 | Native family SSC/Ogg; embedded/external dos, 190/289/381 taps; felys ordering defect declared |
| RE:RAVE | 1 / 2 | Native family SSC/Ogg; LVUP 556 taps, Hard 922 taps; independent selection/origin checks, shared music |
| Happy Rainbow / Decipher | 2 / 2 | Native seven-key SSC/Ogg; 281/609 taps, 38/27 holds, nine/six speed events; visual stop toggles retained in Decipher evidence |
| Destined Marionette / FAKE800 | 2 / 2 | Independent-counter SSC/Ogg; 645/541 taps, seven/one holds; compiled spawn/travel cross-check, measured native movement |


All non-Kouhaku works now use default chart/audio BPM fitting, with a 9 ms
endpoint budget and collision-safe fine-tick fallback. All 19 songs have reviewed
banners. Higan's independently measured 3.917301415 arrow widths/second and
Kouhaku's 2.790697674 remain the base movement, compensated through `#SCROLLS`.
The five saved CW works now normalize to the same SSC/Ogg presentation path;
original effective dos, snapshots and references remain local evidence.
Unsupported 9i is absent from playable exports.

The [default-tempo handover](danoni-default-tempo.md) defines conversion units,
cache/fallback behavior and effect omissions. Straight Soul's nine per-note boosts
are retained in evidence but cannot be represented in SSC. Earlier generation
reports remain archived; current validation includes the original CW parser as
an independent normalization oracle.

## Commands and registry contract

From the research checkout:

```sh
cargo build --manifest-path tools/danoni-dump/pack-verify/Cargo.toml --target-dir target
cc -Wall -Wextra -Werror tools/danoni-dump/ruffle/encode_vorbis.c \
  -I/opt/homebrew/include -L/opt/homebrew/lib -lvorbisenc -lvorbis -logg \
  -o /private/tmp/encode-vorbis
python3 tools/danoni-dump/ruffle/rebuild_corpus.py build \
  "$HOME/Music/DanOni/export-registry.json" "$HOME/Music/DanOni/exports.staging-new" \
  --vorbis-encoder /private/tmp/encode-vorbis --verifier target/debug/ddi-danoni-pack-verify
python3 tools/danoni-dump/ruffle/rebuild_corpus.py promote \
  "$HOME/Music/DanOni/exports.staging-new" "$HOME/Music/DanOni/exports" "$HOME/Music/DanOni/exports-history"
```

Installed FFmpeg is required. The encoder accepts PCM16 mono/stereo; rebuild
uses scientific Python packages for uncached automatic tempo analysis, and no
browser, source JS execution or source network access. See the [analysis command](danoni-default-tempo.md#run-and-evidence). Build the
encoder from the current checked-in source rather than reusing an old binary.
The Rust verifier is a standalone MIT package with local chart/engine dependencies;
it does not change the game crates. It validates all heads/tails, source frame
arrays, source speed/boost arrays for CW, scroll multiplication, preview bounds,
ratings and the selected-chart inventory. Its SHA-256 is recorded in the report.

Registry JSON schema `ddi-danoni-export-registry/1`:

- `root`: absolute external corpus root; `collections`: array of unique
  `{id, title}`; `works`: array of unique work entries.
- IDs are ASCII `[a-z0-9][a-z0-9_-]{0,79}`. Collection titles are safe single
  path components and become the game's displayed pack names.
- Each work has `id`, `collection`, boolean `required`, `kind`, and
  `source: {path, sha256}`. Every binding uses a root-relative, contained regular
  file and SHA-256 of its exact bytes. Pending source bindings are also checked.
- Both recipe kinds require `evidence`: an array of file bindings covering
  the normalized evidence tree or selected CW chart/key/reference/snapshot files.
  Music bytes additionally match the source asset hashes.
- `kind: normalized`: source is `ddi-normalized-work/experimental-1` manifest;
  optional `baseline` binds an accepted prepared manifest. `policy.audio` is
  `ogg` or `wav` (default `ogg`); `musicalGrid`/`nativeScroll`/`artwork` are review bindings;
  `ratings` binds a cached-page numeric difficulty review;
  `displayBpms` maps chart IDs to reviewed display strings; `scrollFactor`
  defaults to 1; `tempoPolicy` is `auto` by default, or `exact`. Generic
  preview/tier defaults come from the shared preparer.
- `kind: cw-dump`: source is a saved dump manifest; `verification` binds its
  successful offline oracle verification. Only complete built-in 5/7/7i/9A/9B
  charts with captured music and matching references are eligible. Custom or
  redefined keys are rejected by the native verifier. Uncaptured playable charts
  in effective dos cannot silently leak through the game's whole-work import.
- `kind: pending`: `reason` is required and `required` must be false. Unknown
  kinds, changed bindings or incomplete required works fail the generation.

`bootstrap ROOT NEW_REGISTRY` is a one-time migration for the historical corpus
layout, not a general discovery command. It copies immutable raw evidence,
accepted pack and reviews before any archival. A relocated motion review keeps
its original bytes separately and copies/hash-checks each measurement inside the
review folder. Source-manifest hash bindings remain unchanged. Higan's measured
review was added explicitly after measurement; bootstrap does not invent one.

## Output and transaction contract

`exports/index.json` has schema `ddi-danoni-export-generation/1`, generation
date, registry/builder/verifier hashes, `sourceRoot`, `networkRequests: 0`,
`browserLaunches: 0`, `passed`, `failedRequiredWorks`, per-work statuses
(`built`, `pending`, `failed`), and hashes for every generated file and ZIP.
Each built work identifies its format, chart/song counts or chart/media bindings,
source path/hash, reference paths and declared omitted effects. Detailed game
parser checks are JSON lines in `validation.jsonl`. The native audio report
`reports/<work>/audio-verification.json` records exact decoded sample counts,
rates/channels and energetic-window correlation against the source PCM. This
checks export origin, not original Flash/device latency. Ogg starts at Vorbis
quality 0.5. Waveform loss alone can trigger bounded retries at 0.7, 0.9 and 1.0;
origin/sample-count failures cannot. Every attempt is reported, the same ≥0.97
correlation and ≤1-sample lag limits apply, and the final encoding command/hash
is recorded. Decipher needs a higher quality; default-passing accepted Kouhaku
bytes stay unchanged.

```text
exports/
  index.json
  validation.jsonl
  kouhaku/紅白FLASH合戦 2004.zip
  kouhaku/files/紅白FLASH合戦 2004/kouhaku--<song>/song.ssc, music.ogg
  single-songs/DanOni Single Songs.zip
  single-songs/files/DanOni Single Songs/higan--normal/song.ssc, music.ogg
  single-songs/files/DanOni Single Songs/<cw-work>/manifest.json, charts/, media/
  reports/<native-work>/manifest.json, songs/, references/, *-review.json
```

Actual SSC filenames are the `simfile` fields in prepared manifests. Collections
contain flat song folders. ZIP filenames equal the collection title: CW
imports use that filename while SSC imports use the internal folder title, so
both assign the same stable pack name. Renaming a ZIP can split that grouping.
Reports/reference PCM never enter playable ZIPs. Music grouping stays within
one source work; no global song deduplication by audio hash. Optional failures
have partial importable files moved under `failed/`, outside collection ZIPs.
ZIP entry ordering, timestamps and permissions are fixed; identical file inputs
produce identical ZIP bytes. Report retrieval/build dates can differ.

`promote` checks the complete file inventory, media/ZIP hashes, required statuses
and native-verifier success before moving anything. It archives a previous
canonical generation and renames the validated stage. Its durable JSON journal
records `stage`, `destination`, `archive`, generation hash and phase. For an
interrupted swap run `recover JOURNAL`: it recognizes and validates a completed
promotion, or restores archived old outputs while retaining the validated stage.
Unexpected changed/ambiguous paths require manual inspection; recovery never
removes a directory.

After successful promotion, `retire OLD_MIXED_EXPORTS GENERATION HISTORY`
verifies the generation, ensures registry bindings no longer depend on the old
mixed tree, records every old file/hash in a retirement journal, then atomically
moves that tree into `exports-history/legacy-mixed-<timestamp>-<id>/`. It deletes
nothing. Interrupted retirement is a single rename: inspect its journal's
`source`/`archive` paths to determine which exists. Historical internal absolute
paths remain historical; current registry and corpus inventory point at the
preserved canonical evidence instead. Failed stages can likewise be archived;
they are never treated as current packs.

## Browser-free extraction direction

The normal SWF extraction path should be **HTTP/cache → HTML link/embed parser
→ restricted native Ruffle → normalized data → export**. Playwright is an
optional discovery fallback, not a required dependency of native SWF conversion.
The inspected ordinary lists already contain child URLs in HTML, including
server-rendered PHP pages and embeds inside `noscript`. iso's page referrer
check affects browser navigation; it does not generate the child links.
Retain the actual parent/link route and honor site/request restrictions.

Known wrappers can be supported declaratively: MFV2's inspected port helper
reads a CSV, and native Ruffle can observe ActionScript-generated sidecar names.
Flash navigation movies need a separate link-inspection/native-navigation
adapter. Arbitrary JS-generated menus can use the optional browser fallback or
remain explicitly unsupported. Do not execute acquired page scripts in Node.

Generating a fresh danoniplus runtime oracle still requires browser execution.
That is a separate verification/acquisition command. Rebuilding exports from
already captured CW references should not launch Chrome. The [index survey](danoni-indexes.md)
contains the concrete examples and archive URL bindings supporting this split.

## What can regenerate now

The registry rebuilds all 15 required works: ten native works (Kouhaku, Higan
and the eight family-adapter additions), plus five eligible saved CW works.
Original Straight Soul is excluded; all other acquired legacy originals
now have validated selected-chart exports. See the [family handover](danoni-native-adapters.md)
for exact contracts, local recipes, diagnostics and coverage.

The native additions use their own movement evidence and exact synthetic frame
clock, generic pickup previews, factual source difficulty labels and null ratings.
Unsupported native effects remain in references and declared loss reports.
The seven newly acquired source manifests remain byte-identical; their root
inventory now points to normalized gameplay evidence. Native nine-key families,
additional/custom difficulties and musical-grid reviews need new proof.

## Original implementation plan and remaining constraints

The following is the original staged plan. Registry migration, cached CW
packaging, all three native family adapters, audio-origin matching and staged
validation/promotion are now implemented as described above. CW normalization,
automated acquisition/navigation and optional artwork remain separate work.

### 1. Local corpus registry and reproducible recipes

Add one local manifest enumerating source works, explicit collection/song/chart
identities, adapter/version or existing CW dump, source/evidence hashes, selection
recipe, supported modes and eligibility. Include the user-supplied Kouhaku SWF;
it currently sits outside the new single-song inventory. Preserve the distinction
between a source SWF, a song, its difficulties and a collection/index.

A recipe must specify optional reviewed BPM/grid, scroll evidence, codec and
shared preview/tier policy. Defaults remain generic. Extraction ratings stay
factual; unknown stars remain null, and arbitrary publisher difficulty numbers
must not be silently treated as Kouhaku-style star counts.

Run from the local cache/evidence by default, with no acquisition during a
rebuild. Emit a result for every registry entry, including unsupported and
failed cases. Record which entries are required for the generation and which
are deliberately pending; an expected pending adapter should not masquerade
as a successful export or disappear from the report.

### 2. Native individual-song projection adapters

For each supported engine family, prove the selection/initialization point,
static lane order, tap/hold arrays, timing transforms and audio origin before
note consumption. Validate all selected difficulties, with 5-key first and
standard 7/7i/9A/9B next. Preserve speed/boost/colour/effect evidence, raw credits,
original dos when present, source key definitions and unsupported behavior.

Suggested order from the observed corpus:

1. Early embedded 2004 iso works, plus the external-dos felys variant.
2. External-dos RE:RAVE and the 2006 festival family, retaining archive bindings.
3. Later ParaFla FAKE800 and the earlier acquired originals; compare Straight
   Soul with its captured port only after checking revision/layout identity.

The existing two adapters pin exact SWFs. Do not weaken their hash checks and
assume a family match. Introduce an explicit tested family contract and add
synthetic cases that exercise that contract alongside opt-in real corpus runs.

### 3. Audio assembly for the new source layouts

The current demux selects exactly one MP3 stream at a reviewed sprite path.
The new single-song SWFs contain multiple stream heads across nested timelines.
Identify the active song's segments, order, start/seek/delay behavior and any
separate event sounds; preserve source compressed payloads and timing metadata.
Do not concatenate every audio block in the movie.

Prefer validated source-stream assembly. If some sources require native PCM
rendering, add a bounded full-song audio mode with sparse state captures and
explicit duration/output limits. The current native worker caps capture/run
iterations at 1,200, so it cannot record a typical complete song at 60 FPS. A
larger bound alone does not prove timer/audio-clock fidelity. Keep the
restricted runner and prove the relevant clock/seek behavior before accepting
that route. Compare decoded sample counts and onset/origin evidence, then use
the existing Ogg encoder and verification policy.

### 4. CW output path: native dump or unified SSC

The newer game already imports saved CW dumps, including nested manifests in a
ZIP. A first generation can package eligible dumps in their existing native
format, with their captured music and per-chart eligibility report. Validate
imports and selected-chart/media associations; do not assume every chart in a
partial work has complete music. This does not need a new normalization adapter
or fresh browser capture.

For a uniform SSC/Ogg pack with the shared preview/grid/scroll policies, convert saved `ddi-danoni-dump/1` references and their selected music into
`ddi-normalized-work/experimental-1`. Check per-chart status, media hashes,
built-in key definitions, chart/music association and oracle identity.

Map frames to **heard-audio-relative seconds** using the recorded engine
settings and observed audio clock. Engine frames may include adjustment,
preblank padding, start position and playback rate; simply dividing every frame
by 60 is not a general proof. Validate the mapping with the runtime state and
known synthetic timing cases. Supply canonical lane order without discarding
lanes. Preserve source colours, boosts, lyrics and other effects separately from
what the chosen export can express.

### 5. Reusable presentation and pack assembly

Reuse `prepare_pack.py`, Ogg encoding, pickup-preview selection, numeric rating
preservation/tier policy, musical-grid bounds and native-scroll compensation.
Add batch orchestration and stable collection/song/chart IDs. Store evidence
and analyses beside each normalized work, independently of generated packs.

The current preparer groups charts by audio hash within one source export.
That must not become global song deduplication: different authors' works can
share audio. Group by an explicit song identity and use hashes to share/cache
media only. Keep legitimate difficulties together and separate songs even when
their audio bytes match.

Before a native-speed-preserving musical export, supply the work's own motion
baseline and author changes. Never reuse Kouhaku's measured baseline for all
songs. An unreviewed musical fit stays an estimate; do not silently apply it or
label synthetic 75 BPM as the recovered musical tempo. Retain the accepted
Kouhaku grid/motion behavior while making per-work decisions explicit.

The current SSC exporter retains source colours/boosts/lyrics in references but
does not encode them all. Each output must declare omissions; some boosts
cannot be represented as a simple global scroll change. Preserving such effects
in playable outputs needs an explicit interchange decision with the importer
session. It need not block a clearly scoped notes/holds/supported-scroll pack.

Artwork is optional for the first clean generation. The existing generic study
can provide reviewed wide banners and a collection image using standard
`#BANNER` and folder conventions. Avoid making source artwork recovery or a
particular illustration style a prerequisite for exporting valid charts.

### 6. Staging, validation and safe replacement

The existing `~/Music/DanOni/legacy-pilot/exports/` mixes original/raw evidence,
normalized source exports, source-hash-bound review JSON, playable variants and
failed experiments. In particular, `2004-kouhaku/`, `autumn-higan-normal/` and
`2004-kouhaku-musical-grid-review.json` must not be treated as disposable files.
The corpus registry currently links Higan evidence inside that directory.

Preserve canonical source/evidence and reviews separately first, checking hashes
and fixing **registry/recipe references**, rather than rewriting immutable
historical evidence. Musical/motion reviews bind to exact source manifest bytes.
The accepted Kouhaku recipe binds to an older adapter export; regenerating with
the current adapter can change that hash. Reuse preserved normalized evidence,
or explicitly rebind only after validating chart IDs, arrays, audio origin and
motion identity. Replacing the stored hash without those comparisons is invalid.

Canonical local layout:

```text
~/Music/DanOni/
  .cache/                              # retained acquisition cache
  corpus/                              # retained CW dumps/oracles
  single-song-corpus-v1/                # retained legacy sources/title evidence
  legacy-pilot/inputs/, captures/, motion/  # retained earlier evidence
  normalized/<work>/                   # immutable normalized source and evidence
  reviews/<work>/                       # explicit analysis/grid/motion decisions
  exports/                             # generated artifacts only
    index.json                         # inventory, versions, statuses and hashes
    kouhaku/紅白FLASH合戦 2004.zip                # one accepted current variant
    single-songs/DanOni Single Songs.zip           # selected, validated individual works
    reports/                           # timing/audio/feature-loss comparisons
  exports-history/<generation>/        # archived old mixed export directory
```

The exact folder names can remain compatible with the current paths if desired;
the important boundary is immutable inputs/reviews versus replaceable outputs.
Pack folder names are display labels and affect imported song identities, so
choose stable names before the first new generation and record the migration.

Build into a new sibling staging directory. Require chart/media integrity,
source/oracle comparisons, hold/collision checks, timing-error limits, measured
scroll behavior, audio sample/origin checks, preview bounds and a complete
required-entry report. Parse the final SSCs with the game's existing parser.
Verify optional artwork/file references and that only 5/7/7i/9A/9B charts are
included. Archive the old tree before promoting the successful staging tree;
retain a rollback location and interrupted-swap recovery. A failed rebuild must
leave the accepted outputs usable. Cleanup is a publication step, not the first
step of extraction.

## Practical implementation sequence

First add the registry, evidence migration and a staged batch rebuild for
**Kouhaku + Higan**, preserving Kouhaku's accepted presentation. Package the eligible saved CW dumps in their existing game-supported format
for an early complete generation of the currently supported subset, or add the
CW normalization bridge for uniform SSC presentation. Then implement/prove the missing native families and audio layouts,
expanding the same registry rather than creating more manually named variants.

This can regenerate a useful supported subset early while making pending works
visible. Regenerating every acquired legacy work faithfully requires the native
projection/audio steps; index crawling and fresh browser oracle capture are not
prerequisites for rebuilding the corpus already on disk.


The initial migration and promotion completed on 2026-10-09. The old mixed tree
was archived at
`~/Music/DanOni/exports-history/legacy-mixed-20261009T201254Z-f39d6376/`:
510 original files were checked against the retirement journal. Development
stages are preserved separately under `exports-history/development-stages-20261009/`.
The live single-song inventory's Higan reference now points to
`normalized/higan/manifest.json`; the original inventory and relocation record
are retained with the supplemental browser report. No source/music files were
committed or deleted. The 2026-10-10 expansion preserves that generation under exports-history and
adds the validated native family works; it does not change accepted Kouhaku
chart/audio bytes or silently accept musical-grid estimates.


The maintainer excluded original Straight Soul from the active registry on
2026-10-10 because startup requires registration. Rebuilding after that removal
preserves both playable ZIPs byte-for-byte; only the work inventory/status changes.
The [presentation follow-up](danoni-presentation.md) records recovered page-star
ratings, unhinted BPM analysis of all ten normalized works and source-informed
cover priorities. Those findings are separate from current playable metadata.


### Artwork generation (2026-10-10)

The [reviewed banner integration](danoni-artwork.md#reviewed-banner-integration)
adds five original Kouhaku selector panels rendered/cropped by native Ruffle,
plus nine plain white-on-black native-work title cards. Song folders use
`banner.png` and SSC `#BANNER`; shared difficulties share a cover. Accepted
Kouhaku chart bytes differ only by that header and its audio bytes are unchanged.
No BPM/rating/preview/scroll policy is changed. Native CW imports still construct
no banner, so five CW songs remain without artwork pending importer support or
the normalization bridge. The only playable nine-key layouts are 9A and 9B;
Hikari's unplayed 9i source row remains local original evidence and can cause the
already documented unsupported-mode warning.


### CW banners and default tempo generation (2026-10-10)

The [offline CW bridge and default BPM fitting](danoni-default-tempo.md) supersede
the artwork-only generation above. All 19 songs now have banners and SSC/Ogg;
all 14 non-Kouhaku songs use fitted musical grids, with original source timing
retained in evidence and endpoint movements below 9 ms. Explicit Kouhaku reviews
and audio remain unchanged. Fresh Chrome import confirms 19 decoded banners,
23 charts and 13,228 judged notes across 5/7/7i/9A/9B, without import warnings.
The source Jam with Pancake display range is retained in the SSC tag; the older
static browser test build displays its fitted 255 BPM summary. Supplemental
browser evidence is local in `reviews/default-tempo-generation-20261010/`.


### Published source-rating generation (2026-10-10)

[Cached-page rating reviews](danoni-source-ratings.md) now supply 15 of the 17
single-song charts with published numerical difficulty. They use Kouhaku's
existing thresholds and put the source count in `#METER`; rated charts omit
`#CHARTNAME` so the menu shows the standard tier instead of a free-text label.
Original labels and alternative author scales remain in the report, including
RE:RAVE's 1–12 progression and カナリア's two author ratings. FAKE800 uses its
current exact-work author-catalogue level 22, retaining the old page's distinct
three-star scale separately. The two festival charts without verified numeric
ratings remain unrated. Chart/audio/presentation timing is unchanged.
