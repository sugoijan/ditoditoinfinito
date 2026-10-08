# DanOni 5/7/9-key corpus and Ruffle extraction work plan

Planning date: 2026-10-08. Status: proposed work; no Ruffle extractor or
SWF-runtime capture has been implemented. This extends the
[CW Edition corpus work](danoni-corpus.md), with 5-key archives as the priority.
The importer/gameplay session still owns the game crates and `docs/PLAN.md`.

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

Keep downloaded SWF execution inside sandboxed Chrome: compile the Ruffle-based
extractor to Wasm and drive its host with the existing Playwright setup. A native
CLI embedding Ruffle would not satisfy the current browser-only execution rule.
Native tools may inspect SWF bytes without executing ActionScript. Start with
Ruffle's existing web host and backends; reducing rendering/audio dependencies
is a later optimization after equivalent extraction has been demonstrated.

## Architecture and boundaries

```text
Public work URL
  -> existing robots-aware capture/cache and original page context
  -> classify HTML5 CW / Flash AVM1 / Flash AVM2 / unsupported
  -> CW browser observer OR Ruffle/Wasm inspection host
  -> exact inputs + native snapshots + engine-specific projection
  -> local corpus validation and importer handover
```

Reuse URL provenance, resource hashes, cache discipline and tagged-value ideas
from the existing dumper. The new observer exports bounded snapshots containing
plain data; it must not invoke getters, methods or string conversions in the
movie to inspect an object. Preserve sparse arrays, undefined values and aliases;
mark values that cannot be inspected. Export no VM pointers or executable code.

Ruffle's host performs only resource requests handled by the capture/cache layer.
Preserve document/SWF base URLs, FlashVars, query parameters and origin behavior;
record emulator settings. Deny unhandled network paths, sockets, mutations and
navigation. Offline execution must have no live-network fallback. Browser
resource routing remains necessary even when emulator networking settings exist.

Keep this tooling under `tools/danoni-dump/`, with an isolated Rust/Wasm build
under `ruffle/` if the feasibility step succeeds. Do not add Ruffle to the game
workspace or change its lockfile. Pin upstream source and patch/build hashes;
check the current release and dependency versions at implementation time. Record
why a pinned upstream snapshot is needed if no suitable stable release exists.
Keep upstream checkout/build outputs outside Git; retain original licences and
add REUSE attribution for any upstream material intentionally committed.

## Work packages and acceptance gates

| Step | Work | Reviewable result / completion gate |
|---|---|---|
| 1. Discovery and classification | Search Japanese sources using `ダンおに`, `5key 作品一覧`, `HTML5移植`, `譜面`, `ParaFla`, `Flash ソース`, and `Ruffle`. Distinguish original publication, current port and restoring publisher. Inspect individual destinations. | Candidate manifest with URLs, modes, engine/AVM evidence, port relationships, terms/robots notes and reasons for selection. No playability claim based solely on an index entry. |
| 2. Library feasibility | Pin Ruffle; build a browser host and a minimal core inspection hook. Use an original synthetic AVM1 SWF with known arrays, holds, sparse values and a post-initialization mutation. | Wasm build runs in Chrome; read-only snapshots match independently specified values before/after the mutation. Repeat offline; enforce frame, elapsed-time and snapshot-size limits. Document patch size and build cost before expanding it. |
| 3. First real legacy adapter | Select one still-public 5-key AVM1 work. Identify its engine family, data loading, chart selection and the point before note arrays are consumed. Use static inspection and observed network requests to locate inputs. | Preserve SWF bytes and external inputs, recover complete lane/hold arrays with explained timing units, and reproduce the result from the local dump. If a complete static chart never exists, report that explicitly. |
| 4. Independent validation | Add a second legacy engine family and, if available, a work with both SWF and HTML5 versions. Compare source-known examples, engine-displayed counts and parsed results. | Explain discrepancies including revisions, offsets, rounding, hold conventions and emulator behavior; do not rewrite expected data to force agreement. A port is corroborating evidence, not automatically identical ground truth. |
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

A small AVM1 fixture running through Ruffle-as-a-library in browser Wasm, with
known arrays exported correctly and repeatably, plus a classified shortlist of
5/7/9-key candidates. This tests the uncertain architectural part before taking
on broad legacy support. No user gameplay testing is required for that milestone.
