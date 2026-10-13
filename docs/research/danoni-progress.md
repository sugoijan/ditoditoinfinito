# DanOni progress and next work

Reviewed 2026-10-13 after rebasing `danoni-research` onto local `main`
`1ab3a9b`. This repository has no `master` branch. All 35 research patches replayed
without conflicts; `git range-diff` reports every patch unchanged. The previous
history is retained at `backup/danoni-research-before-rebase-20261013`.
Use this review for current status; dated acquisition/admission documents retain
historical counts and discoveries.

## Accepted corpus

| Measure | Current accepted generation |
| --- | --- |
| Entire local corpus | 93 songs / 144 charts |
| Kouhaku | 5 songs / 6 charts |
| Single Songs | 88 songs / 138 charts |
| Notes | 96,419 taps / 4,520 holds; 100,939 judged heads |
| Published numeric ratings, Single Songs | 103 rated charts / 35 explicitly unrated |
| Banners | All 93 songs |
| Export registry | 136 sources: 89 required / 47 pending |
| Original v2 inventory | 113 sources: 71 accepted / 3 partly accepted / 39 pending |

These are registered/validated exports, not a count of every candidate or file
on disk. Source entries, songs and chart selectors are different units. Pending
selectors remain in three otherwise admitted works: SUN/鉋モドキ, Yamiyo/Fly-Sky
and 歪んだ神/Death. ESS currently covers only the admitted レク selector; its other
54 menu choices are outside that recipe's coverage.

The playable target remains **5, 7, 7i, 9A and 9B**, with 5-key first. Other
nine-key modes and larger/custom modes stay in source evidence or explicit
exclusions. No additional game modes are proposed here.

Accepted archives remain under `~/Music/DanOni/exports/`. The
[2005 admission](danoni-2005-admission.md) contains the latest generation hashes
and detailed receipts. The source corpus, music and generated images are local
only and are not proposed as distributable assets.

## What the tools already do

The HTML5 CLI captures publicly playable CW work pages in installed Chrome,
records the engine's actual dos/parsed state, caches allowed resources and
verifies an offline replay. Reviewed engine identities are deliberately gated;
syntax capture-point detection alone does not authorize an engine.

Native Flash extraction runs the pinned Ruffle core directly, without WASM,
Playwright or a browser. Its restricted host captures stored AVM1 state,
movie/counter clocks, exact audio starts, PCM and optional rendered pixels.
Family adapters cover child timeline banks, early five/seven-key DAC controllers,
scheduled counter/constant-travel engines and ParaFla. They check source/prepared
array agreement, holds, movement and independently measured audio origins.
Original files and reference frames stay separate from reconstructed metadata.

The shared preparation/rebuild path supplies SSC, quality-5 Ogg, reviewed
numerical tiers, source title cards or original Kouhaku panels, musical-grid
fitting, pickup previews and measured native base scroll. User speed multiplies
that base normally. Applied note/hold endpoints remain within the generic 9 ms
bound; the exact native reference is unchanged. Existing reviewed works can be
rebuilt offline without launching a browser. Promotion archives the old generation
and requires the real game-parser checks to pass.

This is an experimental toolchain with demonstrated family contracts, **not yet
one command that discovers and converts an arbitrary index URL or SWF**. Native
input/proof recipes and several batch-registration/rating-review steps were
prepared with local study scripts. The tracked adapters and rebuild command are
reusable; end-to-end orchestration still needs consolidation. A portable dos
export exists for the original Higan/Kouhaku paths; generic family outputs
currently use normalized JSON plus SSC rather than claiming recovered original
author files.

## Rebase verification and practical limits

The rebased game now requires serde in `ddi-platform`; the research verifier's
separate lockfile was refreshed. The research patches themselves did not need
manual conflict resolution. `just check` passes, including native/wasm/REUSE checks. All **97 Python tests**,
**50 Node tests** and the standalone verifier's regression test pass. The verifier
rebuilt against the rebased game crates accepts **all 89 required entries / 144
charts**, with maximum endpoint difference **8.99839 ms**.
Post-rebase receipts are kept separately under
`~/Music/DanOni/reviews/rebase-20261013/`; they do not rewrite the accepted
export generation or its original capture provenance.

The [post-rebase runtime checks](danoni-runtime.md) now cover the accepted ZIPs
in a fresh release build: all five supported modes on WebGPU/WebGL2, Dito dance
initialization controls, submitted-arrow velocity at x1/x2/x3 and strict-policy
preview checks with the Ogg fallback. DanOni's non-dance symbols correctly use
the procedural renderer. These are selected playback smoke checks; the parser
verification remains the all-chart check. Safari remains manual after publication.

Reconstructed musical BPM/phase is an estimate rather than original metadata.
Numerical catalogue levels and original filled stars can use different scales;
both are retained and the chosen scale is explicit. Native per-arrow speeds,
stair fields, decorative presentation and arbitrary engine customizations are
not perfectly reproduced by global SSC scroll/receptors. CPU-limited converters,
malformed source queues and unavailable resources remain failures/pending cases,
rather than guessed repairs.

## Immediate tooling priority

The maintainer approved the [Rust migration plan](danoni-rust-plan.md) on
2026-10-13: shared library/API and offline preparation first, then audio/DSP,
native extraction/supervision, URL acquisition and HTML5/desktop integration.
The first I/O-free Rust chart-preparation slice is under `tools/danoni-dump/rust/`
and matches all 144 accepted charts through the real game parser, including
quantization diagnostics and authored scroll. Complete offline songs now use the Rust
[preparation API](danoni-rust-offline.md), matching all 93 accepted songs with
recomputed previews and byte-identical supplied media/banners. Native artwork,
collection ZIPs and validated journaled archival/promotion/recovery now complete
the offline milestone, with an isolated 93-song trial. The [Rust audio stage](danoni-rust-audio.md) now decodes supplied sources, encodes
Ogg and checks 79 raw native origins. The [Rust tempo stage](danoni-rust-tempo.md)
now fits lattices/pulses and shared-chart phases with source-bound cache reuse.
The legacy-analysis bridge matches accepted decisions; fresh native estimates
pass timing gates but retain musical alias/phase differences under review.
A generic pulse-resolution/quarter-support refinement improves accepted-family
matches to 117 and explicitly preserves unresolved phases or exact-source fallback.
Accepted exports remain unchanged. The [passive Rust extraction slice](danoni-rust-extraction.md) now projects seven
native families and saved movement/clock evidence. It records two historical
proof gaps shared by current Python instead of weakening checks. SWF demux,
combined artifacts, worker supervision and URL acquisition remain ahead.
The corpus expansion sequence below remains the research backlog; it is deferred
while the accepted 93-song/144-chart generation supplies the migration baseline.

## Corpus expansion backlog

1. **Revalidate the current pack in the updated game — completed 2026-10-13.**
   See the [runtime handover](danoni-runtime.md) and reusable runner. Build a fresh static
   bundle, import both accepted ZIPs into a fresh browser profile, cover all five
   supported modes with the new default skin, and probe x1/x2/x3 native-scroll
   compensation. Verify previews under the user-gesture policy. This checks the
   changed integration before creating another export generation.
2. **Consolidate a reproducible native batch workflow while adding the seven
   remaining embedded-data single-song works.** Start with cached
   `touhouonigiri-tsukimade`, `prologue-babylon`, `otonokogiri-overthef`,
   `hashi-wintersky`, `rakurakusuisui-boss3`, `mfv2-tohosoha3rd` and
   `danoninatu-ophelia`. Acquisition labels them SWF8/AVM1, 60 Hz, five-key,
   with `ni_spdbst`-style identifiers; that is triage, not a proved adapter.
   Observe title, ordinary selection, unconsumed banks, movement/audio and
   requests first. Move repeatable recipe/capture/registration logic into a
   tracked command, keeping third-party data and per-work recipes local.
   Acceptance should include a clean rerun using only tracked tooling and the
   preserved local inputs, plus original synthetic regression cases.
3. **Exercise collections and runtime-dependent loading.** The cached
   `icekirby4-aom-vol1` collection has 12 acquired child songs;
   `matatabi-kantan3` lists five child SWFs. Keep missing children explicit.
   For the two carrotcastle works, observe the exact runtime-built sidecar
   request and nested audio movie before fetching an allowed dependency. Keep
   the query/FlashVars that select the work; shared SWF bytes are not song identity.
4. **Clear the HTML5 engine gates in batches.** There are 24 v2 probes, including
   several distinct v51 host builds and older v18.9.3/v22.5.2 declaration-based
   loaders. Group by exact decoded source hash, review converter/loader behavior,
   then recapture into fresh directories. Reuse the separate cache with a single
   writer. MFV2's reviewed v50 build/port evidence is useful, but the ESS port's
   startup failure and unavailable dependencies must still be resolved naturally;
   no source patch or pretend oracle equality is proposed.
5. **Implement bounded index-page acquisition above those proven adapters.**
   Parse ordinary anchors, frames, preserved embeds and known router parameters
   as data first. Keep browser execution as an optional discovery fallback for
   genuinely script-built navigation and for HTML5 engine reference capture.
   Support explicit Wayback snapshot inputs, mixed-capture provenance and
   missing-resource outcomes. Do not turn an index into an unbounded site crawl.

The remaining 39 v2 sources comprise 24 HTML5 probes, 13 legacy works and two
Wayback recoveries; the registry also retains eight earlier pending CW entries.
NIGHTMARE's pre-compilation transformation, Luvin' You's converter halt, the
partly admitted selectors and archive failures should stay separate diagnostic
tracks. They need explicit evidence/contracts, not progressively weaker gates.
