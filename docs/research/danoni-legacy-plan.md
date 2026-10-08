# DanOni 5/7/9-key corpus and Ruffle extraction work plan

Planning date: 2026-10-08. Status, updated 2026-10-09: native AVM1 inspection and
one pinned real five-key export are verified through CW; broad family support
and original Flash latency remain unproven. This extends the
[CW Edition corpus work](danoni-corpus.md), with 5-key archives as the priority.
The importer/gameplay session still owns the game crates and `docs/PLAN.md`.

## First feasibility result (2026-10-08)

The [native prototype](../../tools/danoni-dump/ruffle/README.md) embeds Ruffle
v0.7.1 at `89a7049965f3090e706265670df309a5128233ac`, independently of the
game workspace. Three minimal hooks expose stored AVM1 properties and VM halt
state. Original synthetic SWFs verify exact before/after note arrays, holds,
sparse/special values, getter non-execution, FlashVars, input and offscreen RGB
pixels. Repeat captures match. macOS restrictions and parent time/memory limits
are exercised; no unconfined fallback is provided. The README records build
cost, dependencies, experimental schemas and reproduction commands.

Synthetic dos round trips through cached CW v48.4.1 preserve integer tap/hold
times exactly. A half-frame note deliberately fails: 240.5 becomes 241, an
8.333… ms error at 60 Hz. Preserve this negative test; accepting `parseFloat`
does not imply that the final CW note arrays retain fractional frames.

Three public AVM1 candidates have cached originals and acquisition provenance:
Higan, Destined Marionette and Straight Soul (5/7-key, with a linked HTML5 port).
The [inventory](danoni-corpus.md#native-legacy-pilot-2026-10-08) separates these
from usable browser references. Higan loads its title screen and both sidecars
offline, but the initial tested input sequence did not advance past the title.
Missing device-font diagnostics were retained; this was resolved in the next
milestone by enabling the bundled default font.

Step 2's inspection/rendering feasibility is established on this macOS host.
At that first milestone, step 3 still needed a verified initialization point and audio-relative timebase.
The prototype's fixed timer steps, wall-clock `getTimer`, and null audio backend
are not a coherent virtual Flash clock. Do not treat `exitCode: 0` or a snapshot
without budget exhaustion as a valid real-song reference. No real-song music
demux, family projection or portable export had been completed then.

## First real result (2026-10-09)

The [native handover](danoni-native.md) now specifies the Higan pilot's complete
projection, observed audio origin, demux and experimental export contract.
Initialization snapshots agree with compiled schedules for 562 taps and 9 holds.
The continuous zero-latency MP3 payload is preserved without transcoding; native
projection and PCM prefix reproduce across fresh restricted runs. CW v48.4.1
comparison preserves all taps, both hold endpoints, 12 speed changes and 342
expanded arrow-colour changes with zero timing error, accounting for its observed
201-frame automatic lead-in. Original Flash device/decoder latency is unmeasured.
This passes the narrow first-work gate, not general engine-family support.

The linked Straight Soul HTML5 port adds two native-mode browser references,
5-key Normal and 7-key Hard, which match offline. The archived Flash version is
acquired but not yet projected; port equivalence remains the next comparison.
The work-level capture remains partial because an embedded statistics document
was excluded and bootstrap resource failures remain recorded.

Independent legacy/browser acquisition caches have been consolidated with the
new [cache merge tool](../../tools/danoni-dump/merge-cache.mjs). The HTML5 port's
small page had been acquired twice through those separate caches; its identical
body hashes and both retrieval dates remain recorded. Subsequent discovery must
use the shared cache. The merge itself makes no requests and preserves refusals.

## Technical basis

Ruffle has a Rust `ruffle_core` library and exports `Player` and `PlayerBuilder`.
Its `avm1` and `avm2` modules are private. Embedding the player therefore does
not by itself provide a public API for walking ActionScript objects. Expect a
small, revision-pinned inspection patch inside the core rather than an external
crate reaching directly into private VM types.
[Sources: core exports](https://github.com/ruffle-rs/ruffle/blob/master/core/src/lib.rs),
[core package](https://github.com/ruffle-rs/ruffle/blob/master/core/Cargo.toml).

The documented web API supports trace observation and explicit
ExternalInterface callbacks, not a general variable inspector. The developer
guide documents AVM1 variable dumping with `avm_debug`; this establishes an
inspection mechanism to investigate, not a verified browser extraction API.
[Sources: PlayerV1](https://ruffle.rs/js-docs/master/interfaces/Player.PlayerV1.html),
[debugging guide](https://github.com/ruffle-rs/ruffle/blob/master/CONTRIBUTING.md#debugging-actionscript-content).

Use a native headless Rust host embedding Ruffle. Wasm and browser bindings are
not required to execute an SWF: Ruffle's test infrastructure already supports
controlled ticks, optional audio/rendering and image comparisons. Its wgpu
`TextureTarget` supports rendering to a texture with pixel readback. Start with
a minimal host and add offscreen rendering for menu diagnosis and screenshots;
retain functioning audio/timer behavior wherever initialization depends on it.
[Sources: native test infrastructure](https://github.com/ruffle-rs/ruffle/tree/master/tests),
[offscreen target](https://github.com/ruffle-rs/ruffle/blob/master/render/wgpu/src/target.rs).

The maintainer accepted native execution for this extractor after the original
browser-only proposal. Run downloaded movies in a restricted worker process,
with read access limited to the captured resources, bounded output and no live
network. Prove those restrictions with synthetic fixtures before real SWFs.
Browser capture still acquires page context; works requiring page JavaScript or
ExternalInterface need an explicit adapter or a browser fallback. Do not assume
that all DanOni engines share the same ActionScript objects or startup sequence.

## Architecture and boundaries

```text
Public work URL
  -> existing robots-aware capture/cache and original page context
  -> classify HTML5 CW / Flash AVM1 / Flash AVM2 / unsupported
  -> CW browser observer OR native Ruffle inspection worker
  -> exact inputs + native snapshots + engine-specific projection
  -> validated CW dos export + local corpus and importer handover
```

Reuse URL provenance, resource hashes, cache discipline and tagged-value ideas
from the existing dumper. The new observer exports bounded snapshots containing
plain data; it must not invoke getters, methods or string conversions in the
movie to inspect an object. Preserve sparse arrays, undefined values and aliases;
mark values that cannot be inspected. Export no VM pointers or executable code.

Ruffle's host resolves resource requests against the capture/cache layer.
Preserve document/SWF base URLs, FlashVars, query parameters and origin behavior;
record emulator settings. Deny unhandled network paths, sockets, mutations and
navigation. Missing resources become an acquisition report for the separate,
robots-aware fetcher; the worker has no live-network fallback. Native host
restrictions and process isolation are both required. Any browser fallback keeps
the current browser resource routing as well as emulator networking settings.

Keep this tooling under `tools/danoni-dump/`, with an isolated native Rust build
under `ruffle/` if the feasibility step succeeds. Do not add Ruffle to the game
workspace or change its lockfile. Pin upstream source and patch/build hashes;
check the current release and dependency versions at implementation time. Record
why a pinned upstream snapshot is needed if no suitable stable release exists.
Keep upstream checkout/build outputs outside Git; retain original licences and
add REUSE attribution for any upstream material intentionally committed.

## Portability goal

Recover static charts for Dito Dito Infinito, preserving every lane, note, hold
and its relationship to the music. Faithful reproduction of arbitrary engine
customizations is not a goal. Extraction may run author code to reach its final
chart arrays; importing or playing the result must not run that code.

Known speed/boost/colour/lyric events can be retained where their semantics are
understood, with game support reported separately. Custom judgement, scoring,
menus, shaders and per-frame gameplay hooks do not become game requirements.
Static custom key layouts may be exported; keys that change dynamically require
an understood representation or an explicit limitation. Never collapse 5/7/9-key
charts into four lanes. A cosmetic omission does not invalidate an otherwise
sound note/timing export. If unknown behavior changes notes, lane identity or
audio timing, mark portability unresolved instead of claiming a usable chart.

Report acquisition, reference validity, chart portability and visual fidelity
separately. This is a proposed additional export assessment; it does not change
the existing dumper's `/1` completeness semantics.

## Output choice: community dos plus preservation and reference data

Use **CW Edition-compatible dos text** as the portable chart output, with JSON
for provenance, key definitions, native references and conversion reports. Keep
the existing CW dump contract unchanged. Legacy captures and exports are
experimental until the pilot establishes their schema; the paths below are the
proposed general export layout. The synthetic-only adapter demonstrates its
chart/dos/key/media subset; it is not the real-work implementation.

Dos is the chart data, not a mandated `.dos` extension or an editor project.
The upstream authoring guide describes `dos.txt` and embedding the same fields
in HTML. Its external `.txt` example is actually JavaScript assigning the data
to `g_externalDos` inside `externalDosInit()`. Thus a file extension alone does
not identify plain dos, and a bare text export cannot simply be passed to the
CW script loader. [Source: authoring and external-data guide](https://github.com/cwtickle/danoniplus/wiki/HowtoMake).

| Artifact | Purpose and proposed representation |
|---|---|
| Existing capture / legacy raw resources | Preserve fetched bytes, charset, inline strings, SWF, media, provenance and hashes. If original dos exists, retain it unchanged, even when its dialect needs conversion. If only compiled/runtime arrays exist, original dos may be unrecoverable. |
| `export/charts/<id>/chart.dos.txt` | UTF-8 plain CW dos for one selected chart, including headers and representable events. A reconstructed export is labelled as such in the export manifest, never called the original source. One chart per directory avoids split/locked numbering and per-chart music ambiguity in the first implementation. |
| `export/charts/<id>/chart.html` | Minimal data carrier with the same dos embedded in `id="dos"`, correctly HTML-escaped. This provides the existing page-based importer a familiar entry point. It is not a standalone player; community users can put the data into a CW template. A generated `externalDosInit` wrapper may be added later if useful. |
| `export/charts/<id>/keys.json` | Declarative lane identities, order and static key definitions, with original mode names preserved. Standard 5/7/7i/9A/9B charts should need no custom script. Custom layouts need an explicit CW definition mapping before they are called CW-playable. |
| `export/media/<hash>.<ext>` | Shared local audio and retained optional images, referenced by relative paths. Record extraction/transcoding steps; do not replace missing music with silence. |
| `export/manifest.json` | Versioned export index linking stable source chart IDs, original difficulty labels/modes, local paths, credits, capture identity, hashes and preserved-versus-reconstructed status. Different songs in a collection retain their own music association. |
| `export/conversion.json` | Per-chart field mappings, timing conversion, omitted/unknown features, portability result and round-trip comparison, including measured error. Missing evidence is distinct from a known omission. |
| Native/reference JSON in the capture | Evidence of what the observed engine computed, with engine identity, units, capture stage and original values. Legacy results must not pretend to be a CW `scoreConvert` return. The export links to this evidence; it does not overwrite it. |

Multi-chart works retain their source IDs and ordering in the manifest even
though each exported chart is independently numbered as the first chart in its
dos. Preserve source `musicTitle`/`tuning` and original/restoring authors; do not
infer authorship from the host. Exported third-party material remains local under
the same corpus rules as the raw captures.

Prefer recovered source text when its semantics are known. For legacy inputs,
use explicit dialect mappings: the upstream converter already translates old
headers, colour targets and modes such as `DP` to `9A` and `9` to `9B`. This is
more than replacing `&` with `|`.
[Source: CW migration converter](https://github.com/cwtickle/danoniplus-converter).

When reconstructing dos from runtime values, establish the mapping to audio
time first. Parsed frames may already include blank frames, author adjustment,
playback-rate conversion and automatic lead-in. Copying those frames into dos
while keeping source offsets can apply the adjustments twice. Record source
timebase, audio origin, transformations and chosen CW settings, then load the
generated data through a pinned CW engine and compare lane identities, counts,
tap times and both hold endpoints against the captured audio-relative values.
Compare supported effects too. Report maximum timing error and any rounding;
never silently quantize an unrepresentable chart or equate SWF header FPS with
the chart's clock. Establish representable precision in the synthetic tests
before accepting a real export. This round trip checks the conversion, not
perfect emulation of the original Flash player.

Authoring-project recovery is a separate, optional task. The community's
Chart Reverser converts chart text to editor save data but asks for first-number,
BPM and tempo-change information; its documentation warns that wrong timing
inputs can reconstruct different grid positions. Runtime note frames do not
uniquely recover that authoring information. Do not promise the original editor
project. [Source: Chart Reverser](https://github.com/cwtickle/danoniplus-reverser).
Use it as an additional reference for source-known first-number/BPM cases;
agreement is evidence about that reconstruction, not an independent proof of
unknown Flash timing or exact recovery of the author's original project.

Do not add SM/SSC export in this work: it would require another mapping for
DanOni key layouts and effects without improving compatibility with the DanOni
importer. The game session owns any importer extension for the proposed export
manifest or declarative custom keys; record those needs here before stabilizing
the schema.

## Work packages and acceptance gates

| Step | Work | Reviewable result / completion gate |
|---|---|---|
| 1. Discovery and classification | Search Japanese sources using `ダンおに`, `5key 作品一覧`, `HTML5移植`, `譜面`, `ParaFla`, `Flash ソース`, and `Ruffle`. Distinguish original publication, current port and restoring publisher. Inspect individual destinations. | Candidate manifest with URLs, modes, engine/AVM evidence, port relationships, terms/robots notes and reasons for selection. No playability claim based solely on an index entry. |
| 2. Library feasibility | Pin Ruffle; build a native restricted worker and a minimal core inspection hook. Use an original synthetic AVM1 SWF with known arrays, holds, sparse values and a post-initialization mutation. | Read-only snapshots match independently specified values before/after the mutation; an offscreen screenshot proves the rendering path. Repeat offline; test denied resource access and frame, elapsed-time, memory and snapshot-size limits. Document patch size and build cost before expanding it. |
| 3. First real legacy adapter | Select one still-public 5-key AVM1 work. Identify its engine family, data loading, chart selection and the point before note arrays are consumed. Use static inspection and observed network requests to locate inputs. | Preserve SWF bytes and external inputs, recover complete lane/hold arrays with explained timing units, and reproduce the result from the local dump. If a complete static chart never exists, report that explicitly. |
| 4. Export and independent validation | Produce CW dos from the first supported family and round-trip it through CW. Add a second legacy family and, if available, a work with both SWF and HTML5 versions. Compare source-known examples, engine-displayed counts and parsed results. | Validate audio-relative notes/holds and understood effects; document unsupported custom behavior. Explain revisions, offsets, rounding and emulator discrepancies; do not rewrite expected data to force agreement. A port is corroborating evidence, not automatically identical ground truth. |
| 5. Expand the corpus | Grow the HTML5 batch alongside steps 2–4, then add supported legacy examples. Select for missing features and publisher/engine diversity. | Every selected chart has explicit data/reference/music/visual/replay results, provenance and coverage tags justified by captured state. Unsupported candidates remain in the inventory with reasons. |
| 6. Stabilize the interchange | Turn proven AVM1 observations into an engine-specific schema and adapter tests. Produce a corpus report and importer integration notes. | Document units, lane identity, capture stages, native fields, unsupported effects and deterministic offline comparisons. Run tool tests and `just check`; commit only tooling, documentation and permitted fixtures. |

Steps 2–4 are feasibility gates, not a promise to support every SWF. If VM
inspection needs a broad fork, the target uses unsupported AVM2 behavior, or the
movie generates its chart incrementally, preserve the evidence and narrow the
supported family. Continue the independent HTML5 corpus work.

## Corpus selection

The first expansion target is 12 HTML5 works: roughly eight 5-key works and
four works covering 7, 7i, 9A and 9B. Mixed-mode works can cover several cells.
Aim for at least three publishers and a mixture of native HTML5 releases and
older ports. Capture all relevant difficulties where practical, documenting
subsets to keep downloads bounded. These are selection targets, not verified
available counts. The legacy pilot is two or three AVM1 works, initially 5-key.

Primary discovery routes:

- [MFV2's 5-key catalogue](https://cw7.sakura.ne.jp/lst/ds0301/?keys=5):
  candidates include Holy Moon, TeamSaboMFV2 and Cross Galaxy. The index marks
  many HTML5 ports. These links are candidates, not completed captures.
- [MFV2's HTML5 all-chart list](https://mfv2.sakura.ne.jp/do/ALL_List.php) and
  [restoration explanation](https://mfv2.sakura.ne.jp/do/DanOniTransplant.php):
  record original authors and the restoring publisher separately. The publisher
  explicitly restricts direct MP3 access; follow the normal browser path and
  retain refusals. Do not assume that every restoration has identical permission
  or revision history.
- [9Akey Package](https://apoi108.sakura.ne.jp/danoni/9Akey_package/): a targeted
  source for multiple difficulties and holds. Distinguish modes using author key
  definitions, as in the [organizer's key guide](https://cw7.sakura.ne.jp/event/nfes20/keys.html).
- [Crepe Curtain](https://ccsyouta.sakura.ne.jp/about.html) describes both Flash
  and HTML5 works; [AutumnLeaves](https://autumnleaves.yukishigure.com/danoni.html)
  is another legacy candidate list. Verify the actual page and resources before
  calling any SWF playable. Removed or restricted content stays out of scope.

Coverage priorities: the 5-key onigiri lane and its holds; multiple difficulties;
speed/boost/colour changes; external and inline inputs; Shift_JIS; split/locked
numbering; lyrics; custom key definitions; embedded/external music; fractional
timing; and differing engine generations. Preserve 7 versus 7i and 9A versus
9B identities. Native lane playback is a separate requirement for the game
session; the extractor must not discard lanes to fit the current 4K game.

## Legacy reference contract to prove before standardizing

Do not label emulated Flash results as `danoniplus` or force absent CW fields
such as `difData` into invented values. The existing `/1` CW schemas remain
unchanged during the pilot. Use an explicitly experimental legacy output until
real captures establish its shape; later choose a new schema version or a
separate reference type with a common lane/event projection.

Required evidence for each legacy result:

- Original page/SWF URLs and hashes, external data, original and restored credits.
- AVM generation, identified legacy engine family, Ruffle revision/build/patch
  hashes, settings and adapter identity.
- Capture point and input sequence; native object paths and observed values;
  pre-consumption versus later runtime state. Static text can be absent when
  the movie constructs arrays directly; do not synthesize an alleged original dos.
- SWF header frame rate, observed engine timebase and conversion to seconds.
  Do not assume these are equal or that every Flash source uses CW's 60 Hz model.
- Taps, flat/pair hold data, lane identity, speed/boost/colour events and other
  effects actually understood by this adapter, with unknown fields retained.
- Separate statuses for raw acquisition, valid parse reference, music, visuals,
  offline reproducibility and unsupported dynamic behavior.

Embedded sound extraction, if needed, must preserve the original SWF and record
sound/tag identity and codec/framing transformations. Audio duration or a screen
recording cannot establish chart timing. No fabricated silence in real dumps;
synthetic silent audio is confined to original test fixtures.

## First deliverable

A small AVM1 fixture running through native Ruffle-as-a-library, with known
arrays exported correctly and repeatably, a verified restricted host and an
offscreen screenshot, plus a classified shortlist of 5/7/9-key candidates.
Then prove reconstruction and CW round-trip timing on synthetic charts before
exporting a real 5-key work. No user gameplay testing is required for these
milestones. Broad custom-engine playback remains outside the goal.
