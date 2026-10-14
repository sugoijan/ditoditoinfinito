# URL-to-prepared-song bridge

The native Rust pipeline now joins [URL extraction](danoni-rust-url-native.md)
to [audio preparation](danoni-rust-audio.md), [fresh BPM estimation](danoni-rust-tempo.md),
title artwork and [complete-song assembly](danoni-rust-offline.md). It writes
candidate SSC song folders from the currently supported native engine families.
A successful result is ready for host inspection/import; musical acceptance and
installation in the desktop library are separate operations.

## APIs

`preparation::prepare(&Request, &impl AssetSource, control)` is the reusable,
I/O-free coordinator. It returns the existing `work::PreparedWork`: manifest plus
contained, hash-inventoried artifact bytes. Its camelCase request has:

- `manifest`: exact normalized source `Binding`.
- `motion`: exact `Binding` of the producer's source-bound native scroll review.
  Complete chart coverage and any bound measurement evidence must validate.
- `font`: optional explicit host font `Binding`, at most 32 MiB. The font is used
  to render plain-title banners, with its hash in artwork evidence; its bytes are
  not distributed. No font means no generated banner, recorded explicitly.
- `options`: defaults to `codec: "ogg"`, `quality: 0.5` and the existing default
  `work::WorkOptions` (stars classifier, x1 correction factor, pickup previews).
  `wav` is supported. Ogg requires the `vorbis` feature; a build without it returns
  `encoder-unavailable`, rather than choosing another codec silently.

The coordinator verifies and caches host bytes across stages, at most 512 MiB;
generated stage inputs have a separate 512 MiB / 16,384-file bound. Consumers
continue to verify exact bindings. `ddi-preparation` is a reserved generated
namespace. Progress stages are `validate`, `audio`, `tempo`, `artwork`, `songs`,
`complete`; any callback can return an error. Source data is borrowed unchanged.

`url_prepare::Request::new(url)` supplies URL and preparation defaults. Call
`url_prepare::execute(request, cache, worker, host_assets, fresh_output, cancel,
site, progress)` on a blocking job thread outside an entered async runtime.
Host-owned services remain the trusted worker installation, access decisions,
cache/output locations, optional font bytes, cancellation and progress. Nothing
in this entry point requires Playwright, Python, Node or FFmpeg. Work scripts
continue to run only inside the existing offline confined native worker.

Its camelCase request contains:

- `source`: existing `url_native::UrlRequest`, including public URL, optional
  selector subset, metadata and bounded HTTP/profile/capture options.
- `font`: optional host binding; namespaces `source` and `ddi-preparation` are
  reserved. The source directory cannot redirect reads into host font namespaces.
- `preparation`: the `preparation::Options` above.
- `wallMs`: whole-job cooperative deadline, default/max 300,000 ms. The child
  source operation also retains its own shorter HTTP/extraction bound.
- `outputBytes`: whole-job cooperative disk bound, default/max 2 GiB. It covers
  source evidence, intermediate native runs, prepared artifacts and final receipt.
  This is checkpoint polling, not a filesystem quota or a hard memory limit.

Progress has `stage` (`source`, `preparation`, `writing`, `ready`) and optional
nested `source` or `preparation` progress. A nested source `ready`/preparation
`complete` is not whole-job readiness. `NetworkCancel` is checked before and after
host callbacks, during existing audio/tempo loops, source requests and worker
execution, during file writing, and immediately before final publication.

## Default policies

Each distinct normalized source audio hash is decoded and encoded once. Charts
sharing that music share playback, PCM, preview and banner. WAV/Ogg preparation
uses the already measured normalized origin: this stage adds no musical offset,
trims no leading silence and performs no new original-Flash latency estimate.

Fresh native tempo fitting always runs over the source note heads and prepared
PCM, using the existing versioned lattice/audio/quarter-phase policy. Every
accepted grid must still pass full endpoint, lane-relationship and speed-event
checks. Fits retain the existing 9 ms endpoint budget. Their musical quality
remains **provisional**; neither a valid lattice nor a Ready job approves a BPM or
beat phase as musically correct. The full analysis is saved for review.

If fitting returns `exact-source-fallback`, no partial remapping is applied:
source-clock ticks at 75 BPM preserve timing. For charts without authored display
BPM metadata or explicit host display overrides, this coordinator supplies
`#DISPLAYBPM:*` so 75 is not advertised as the music's measured BPM. Native motion
remains applied in this case too. Authored/host display metadata retains precedence.

The source-bound observed arrow-heights/second measurement determines `#SCROLLS`:
`baseRate * scrollFactor * 60 / timingBpm`, multiplied by source speed events.
The player's x1/x2/x3 multiplier still applies on top. Fits change the beat grid
and note colouring within the timing budget; they do not substitute BPM-derived
velocity for the measured default native velocity.

Source stars use the existing inclusive `[1,3,5,7]` classifier. No free-text label
is converted into a numeric difficulty. Missing stars remain unknown (Edit/0);
this coordinator does not guess ratings from a page or blur unrelated scales.
The existing PCM/note-density pickup algorithm chooses previews with no per-song
parameters. Supplied host fonts render shared 640×200 plain-title banners from the
same metadata used for SSC headers. Engine-specific panel capture remains an
independent producer.

The passive page parser now retains one completed HTML `<title>`, normalizing
whitespace and decoding HTML entities. Multiple/incomplete titles or titles over
512 characters are ignored. When host title/musicTitle metadata is absent, URL
extraction records that title and a bound discovery/body-hash citation as fallback
metadata. It is explicitly an HTML work-page title and can include site numbering;
it is not a new artist credit. Source page URLs are also retained in the normalized
manifest. Direct SWFs without title metadata still fall back to chart identifiers.
No work script is evaluated to infer titles or ratings.

## Layout and receipts

```text
new-output/
  source/                  complete url_native output and immutable acquisition evidence
    manifest.json          ddi-danoni-url-native-run/1
    acquisition/...
    native/...
  prepared/
    manifest.json          ddi-danoni-prepared-work/1
    songs/<song-id>/
      song.ssc
      music.ogg            or music.wav
      banner.png           when an explicit font was supplied
    references/...         PCM and source chart/reference preservation
    provenance/...
    provenance/preparation.json
  manifest.json            ddi-danoni-url-prepared-run/1, success only
  failure.json             failure instead of a parent readiness marker
```

The prepared-work schema and replay bindings are unchanged, with additive
`preparation: {path, sha256, tempoStatus, musicalQuality: "provisional"}` and the
new report included in its ordinary artifact inventory. The report schema
`ddi-danoni-fresh-preparation/1` records the exact request/source manifest,
complete tempo result, motion/artwork/rating policies and `libraryInstalled:false`.
All source times and previews are seconds, note deviations milliseconds, native
motion arrow heights/second, and SSC rows 48 ticks/beat as before.

The outer `ddi-danoni-url-prepared-run/1` contains `ready`, exact `request`,
`failure` (null or stable error), `scope`, and null or:

```json
{
  "completed": {
    "source": {"path": "source/manifest.json", "sha256": "<64 lowercase hex>"},
    "prepared": {"path": "prepared/manifest.json", "sha256": "<64 lowercase hex>"},
    "songs": 1,
    "charts": 2,
    "musicalQuality": "provisional",
    "libraryInstalled": false
  }
}
```

All paths are relative to the outer directory. Each child receipt binds its own
inputs and artifacts. The source child carries original retrieval/policy records,
URLs, bytes and native proofs; the prepared child carries song data and the full
fresh fitting diagnostics. No third-party input is placed in the repository.

The complete outer receipt is serialized and staged before the final `ready`
callback. Only success after that callback publishes `manifest.json`. Late
cancellation removes the pending receipt, leaves `completed:null` in `failure.json`
and retains child diagnostics even when both child manifests already exist.
Callers must use the **outer** manifest as their import candidate marker. Existing
exports, registry and library are never overwritten. This marker has the existing
fresh-output guarantee; installation/crash recovery still belongs to a future
host library transaction.

## CLI

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- \
  prepare-url-native /external/trusted-worker-bundle \
  /external/job.json /external/new-candidate
```

Job paths stay outside Git. Example (replace the URL/font hash with real values):

```json
{
  "schema": "ddi-danoni-url-prepared-job/1",
  "cache": "cache",
  "roots": {"font": "fonts"},
  "origins": {"https://example.org": "Host-reviewed anonymous public work access"},
  "request": {
    "source": {"url": "https://example.org/work/song.html"},
    "font": {"namespace": "font", "path": "font.ttf", "sha256": "<font SHA-256>"},
    "preparation": {}
  }
}
```

`roots` defaults empty and resolves relative to the job file; `font:null` needs
no font root. Site decisions grant no robots exemption. HTTP caching, explicit
resolver/trust selection, archive boundaries, no automatic GET retry and known
engine/runtime admission remain the source bridge's policies. Set
`request.source.fetch.offline:true` to require existing complete cached evidence.
The installed worker, global font/config and cache are host setup; no per-song
capture recipe or precomputed BPM grid is required.

## Validation and remaining scope

The synthetic joined-pipeline gate checks shared-music grouping, native fresh
fits and insufficient-fit fallback, all endpoint timing through the real game
parser, source motion at x1/x2/x3, original stars, single host reads, artifact
hash coverage and cancellation at preparation stages. The optional real-worker
`tests/url_prepared.rs` gate uses an offline prepared-URL job, verifies every
prepared artifact and endpoint through the game parser, compares x1/x2/x3
velocities against source motion, then cancels at final Ready after both child
manifests exist. Configure `DDI_DANONI_PREPARED_URL_JOB`,
`DDI_DANONI_PREPARED_URL_OUTPUT` (new directory) and
`DDI_DANONI_WORKER_BUNDLE`; absent local data skips this gate.

The local 2026-10-15 checkpoint exercises Higan and the No Name/灼冬赫夜 page:
two songs / three charts, 2,092 source endpoints, maximum deviation
8.5072650759 ms. Fresh fits are approximately 154.99 and 148.999875 BPM,
respectively; these are recorded estimates, not newly accepted corpus decisions.
Both jobs render plain-title banners from an explicitly supplied local font,
produce Ogg playback, pass final-cancellation checks and rebuild every core song
artifact using only their own prepared input inventory. The CLI independently
completes Higan. All runs use zero GETs and separate cache copies.

The local review root is `~/Music/DanOni/reviews/rust-url-preparation-20261014/`:
`higan-v2/success`, `noname-v2/success`, their corresponding `late-cancel`
directories and `higan-cli` contain the final outputs; earlier title-metadata
failures are retained separately. Validation binds source/lockfile hashes,
receipts, logs and unchanged original cache/export/registry hashes. The existing
93-song / 144-chart accepted generation is unchanged.

Checks pass 217 native and 88 feature-disabled SDK tests, strict Clippy in both
configurations, formatting and `UV_OFFLINE=1 just check` (838/838 REUSE). The
1,629-request/robots cache oracle (941 bound files read once) and 91 cached-page
cases pass. Real-worker and prepared-only replay gates run separately with the
local environment above; absent those variables the ordinary suite skips their
external corpus execution. End-to-end execution is currently macOS-only: the
supervisor refuses other platforms until their confinement adapters are proved,
and persistent cache writing still requires its existing tested Unix primitives.

Current source support remains the proven native profile subset, especially
5-key Reku families and the supported 7-key family. Higan's automatic colour
triples remain raw/unmapped presentation, as documented in the source handover.
Optional-resource review, colour mapping, broader native families, HTML5 control,
index/frame traversal and richer source metadata are separate work. No claim of
new 9A/9B extraction or perfect engine-customization reproduction is made here.
Desktop library installation and cancellation/crash recovery across that library
boundary are the next integration contract; this slice changes no app/library code.
