# Regenerating the local DanOni exports

Planning date: 2026-10-09. The maintainer wants one clean export generation from
the updated local corpus, including Kouhaku. This records the implementation
needed before replacing outputs; no existing source, review or export was moved
or deleted while preparing this plan.

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

| Input | Present implementation | Remaining work |
| --- | --- | --- |
| Kouhaku | Pinned native adapter, audio demux, normalized export, reviewed musical grid/motion, shared pack preparation | Register the accepted recipe and immutable evidence/reviews; preserve/revalidate hash bindings when using the current adapter |
| Higan Normal | Pinned native adapter and validated note/hold/speed/colour reference, demuxed audio | Register the recipe, prepare a current playable pack, and supply a verified motion baseline before claiming native-speed preservation |
| New CW works: Dancing at Dusk, カナリア, ふしぎなひかり 9A/9B | Four complete selected-chart oracles/media captures, verified offline | Unified SSC path: CW dump → normalized-work adapter and verified timing/motion mapping. Existing native-format dumps can be packaged sooner. |
| Earlier complete usable CW captures, including Jam with Pancake and Straight Soul | Existing oracles/media; some work-level limitations retained | Per-chart/media eligibility checks; normalization only if targeting the shared SSC presentation |
| Seven new individual-song SWFs | Original pages/movie/sidecars, successful native title captures | Initialization/selection recipes, source-family chart projection, audio assembly/origin, movement proof and independent validation |
| Destined Marionette and original Straight Soul | Acquired earlier; original SWFs not projected | Same native work; Straight Soul's HTML5 port is corroboration, not an assumed identical chart |
| Other/custom modes or incomplete dependencies | Evidence may exist | Explicit skipped/unsupported/partial entries; never invent missing music or remap to a supported mode by lane count |

The immediately regenerable native set is **six songs/seven charts**: Kouhaku's
five/six and Higan's one/one. This does not mean all seven newly acquired SWFs
can already be converted. They and the two earlier unprojected originals make
**nine pending legacy projection cases**, potentially sharing a smaller number
of engine-family adapters. Family compatibility must be proved.

## Required implementation

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
iterations at 600, so it cannot record a typical complete song at 60 FPS. A
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

Suggested canonical layout (new directories are proposed):

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
    kouhaku/playable.zip                # one accepted current variant
    single-songs/playable.zip           # selected, validated individual works
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
