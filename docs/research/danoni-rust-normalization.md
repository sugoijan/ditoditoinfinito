# Rust saved-run normalization

Implemented 2026-10-13 as the combined-artifact slice of
[milestone 4](danoni-rust-plan.md#4-native-extraction-and-supervision).
`extraction::prepare_native_work` joins saved structural chart, motion and audio
runs, inventories the actual movie, demuxes unchanged MP3 frames, measures native
PCM alignment independently for every chart, and produces a portable normalized
work plus native-scroll evidence. It runs in process without Python, Node,
FFmpeg, a browser, network access or execution of a work's scripts.

This is **saved-run normalization**. It does not launch Ruffle, discover selectors,
fetch a URL, fit musical BPM, produce a banner/rating review or install songs.
The separately pinned Higan/Kouhaku chart contracts are still pending; their
static audio demux is already checked. Accepted exports remain unchanged.

## Host API and request

```text
prepare_native_work(&NativeWorkRequest,&AssetSource,checkpoint)
  -> NormalizedWork {manifest: Value,receipt: Value,files: Vec<Artifact>}

NativeWorkRequest (camelCase, unknown fields reject):
  sourceManifest: Binding
  movie: Binding
  metadata: {textKey: textValue,...}    default {}
  charts: NativeChartRequest[]

NativeChartRequest:
  id: ASCII letter/digit/underscore/hyphen string
  label: string | null
  options: ProjectionOptions
  projection: CaptureBindings
  motion: CaptureBindings
  motionSelection: MotionSelection
  audio: CaptureBindings
  nativePcm: Binding
  streamStartIndex: unsigned integer
  optionalColor: OptionalColorEvidence | null
```

`Binding`, `CaptureBindings`, supported families/modes, `ProjectionOptions`,
`MotionSelection` and the narrowly reviewed missing-color exception use the
[passive extraction contract](danoni-rust-extraction.md). Every binding names an
explicit namespace, contained relative path and lowercase SHA256. No directory
scan, implicit cache lookup or path fallback occurs in this API.

There must be 1–128 charts, with unique nonempty IDs of at most 128 ASCII bytes;
labels are at most 8,192 bytes. Caller metadata has at most 128 text entries, with
128-byte keys and 8,192-byte values. Five/seven structural modes remain explicit.
Each **requested** chart must pass; failure returns no partial work. A caller can
request a deliberate subset, which remains visible in `request.json`. The corpus
trial below declares all excluded selectors separately, rather than presenting a
subset as complete extraction of a work.

Source bytes are checked using the existing 512 MiB total input budget: movie
16 MiB, decompressed SWF 64 MiB, snapshot 64 MiB, run/config/source manifest/
review 16 MiB, other assets/native PCM 128 MiB. Generated JSON is at most 16 MiB;
output artifacts total at most 512 MiB / 32,768 files. Source and derived artifacts
use shared immutable buffers where possible. These are input/output bounds, not
an operating-system RSS quota. Run synchronous parsing/DSP off the desktop UI
thread; cancellation checkpoints run through chart, tag/frame, codec, correlation
and assembly stages.

## What the join proves

The original source manifest's SWF hash must match actual movie bytes. All three
captures independently pass movie/snapshot/parent/config hashes, pinned Ruffle
revision and successful unchanged-run checks. Configured movie paths resolve
from each bound config to the explicit movie binding. Every declared cached
resource is hash-checked using its config-relative path; duplicate URLs reject.
The original source manifest is retained verbatim, including its acquisition
provenance. Normalization does not independently revalidate HTTP/robots receipts.

Worker and sandbox SHA256 identities must be present and identical across the
three parent reports. This checks saved identities, not the binary/profile files;
the fresh supervisor must later bind/package those actual implementations.

Configs must otherwise be identical after removing only `captureFrames`,
`render`, `mixAudio` and `avmExecutionMs`. These describe observation, rendering,
mixing and execution budgets; successful nonhalted runs can use different values.
Movie/source/page identity, selector events, resources/charsets and unknown script
settings remain part of the comparison. No event is dropped just because it looks
like a presentation option. A mismatch returns `native-run-controls-mismatch`;
worker/sandbox mismatch returns `native-run-identity-mismatch`.

Projection proves complete source/native banks or schedules under its family
contract. Motion proves source arrival, geometry and stored author speed units.
Audio-start state must have the same logical counter offset as every motion
sample. The actual SWF slice must match the observed head, FPS and sample budget.
Native PCM must be exactly 44,100 Hz stereo signed PCM16, with captured frame
count and an enabled mixer, before correlation is attempted.

The combined producer joins the established decoration exception as follows:

1. The family projector generates diagnostics only after proving completed
   conversion and required selected-mode receptor sprites.
2. Cross-run script inputs/selectors/resources and worker identities must match.
3. Motion/audio error arrays must equal the proved diagnostic array **in full**,
   including multiplicity, order, iteration, message and truncation fields.
4. The normal complete/nonhalted audit gate still checks every remaining entry.

Thus the 16 audio-only jobs previously rejected as `native-audio-audit-unproved`
can pass here using independently bound earlier snapshots. Extra identical errors,
changed iterations/messages or incomplete audits fail with
`native-decoration-proof-mismatch` or the normal audit error. Source audits are
never edited to hide the errors; original bytes and retained diagnostics are part
of the normalized evidence. The standalone `demux-native` job stays strict.
The optional missing-color 404 review remains separate and explicitly bound in
all three captures; original color diagnostics are retained per run.

## Audio and shared songs

Demux retains whole encoded MP3 frames and per-chart stream reports. Decode uses
Symphonia; comparison PCM uses the already verified Rust resampler at 44,100 Hz
stereo. Correlation uses the captured output interval, bounded by the next stream
start where present, under the [decoder-origin policy](danoni-rust-audio.md).
The measured lag is converted to source-rate sample frames by ties-even rounding.
Positive values trim lead-in; negative values prepend silence. No note-grid or
musical phase enters this calculation.

MP3 decode may be cached within a work. **Every chart** still independently
correlates its own bound native PCM and retains its origin report. A shared MP3
hash must have identical logical music-start frame and canonical WAV hash;
otherwise `native-shared-audio-origin-mismatch` rejects the work. A stream report
is stored per chart, so deduplicated music does not erase distinct stream slices.

New WAV headers/decoder rounding can change hashes relative to FFmpeg output.
The corpus checks exact rate/channel/frame counts and a maximum difference of
one signed PCM unit, as well as unchanged measured shifts. Original MP3 and raw
native PCM remain available. This stage writes WAV only; subsequent
`audio::prepare_media` can encode Ogg and produce the existing neutral receipt.

## Precise artifact layout

```text
manifest.json                         final normalized-work readiness marker
normalization.json                    new joined-proof receipt and inventory
request.json                          exact typed request, no external root guessing
source-manifest.json                  original byte-identical acquisition manifest
native-scroll-review.json             review bound to these manifest bytes
references/<chart>.json               source projection, motion, metadata and origin
references/<chart>-motion.json        measured native motion; review evidence
references/<chart>-stream.json        original/selected SWF block and seek metadata
references/<chart>-audio-origin.json  measured Rust decoder-origin report
media/<mp3-sha256>/source.mp3          unchanged concatenated encoded payload
media/<mp3-sha256>/source-decoded.wav  complete decoded source-rate PCM16
media/<mp3-sha256>/music.wav           canonical measured-origin PCM16
evidence/inputs/<sha256>             every bound original input, deduplicated by hash
```

No acquired files live in Git or fixtures. The `evidence/inputs` map preserves raw snapshots,
parent reports, configs, movie, resources, native PCM and explicit color receipts.
It contains the exact required bound inputs, rather than copying arbitrary trees
or following unrelated file paths from an acquisition manifest.

`manifest.json` uses `ddi-normalized-work/experimental-1`, adapter
`native-rust-families/1`, and fields:

```text
sourceManifestSha256, sourceSwfSha256, sourcePageUrl, capturedAt
metadata: original request metadata
charts: normalized chart objects[]
normalization: "normalization.json"
assessment: source/reference/audio/conversion scope
```

Charts preserve original family fields, frames, schedules, native keys, events,
source-order issues and diagnostics. Audio-relative seconds are
`(nativeFrame-logicalAudioOrigin)/60` for taps, hold heads/tails, speed, boost and
visual stops. Each chart additionally names `id`, canonical
`music:{path,sha256,mimeType:"audio/wav"}`, `audioOrigin`, `nativeReference`, merged
`metadata`, `difficultyStars:null` and `effects`. Caller metadata/labels and the
existing native/source artist fallback retain Python behavior; source difficulty
text does not become a numeric rating. Original publisher headers/credits remain
in the projection and raw snapshots. Existing source-bound rating/artwork stages
can supply their independent reviews later.

Effects retain inactive speed tails, the global SSC approximation of native
per-arrow-at-spawn speed, missing optional colors, unmapped native presentation,
inactive lane schedules and cursor-order issues. They are not silently converted
into supported engine customization. The reference schema remains
`ddi-native-family-reference/experimental-1`, with `normalizationPolicy:
"native-normalization/1"` and the original source/timing limitation statements.

The per-chart audio report uses `ddi-danoni-native-audio-origin/1`, producer
`native-origin-normalization/1`:

```text
sourceMp3Sha256, nativePcmSha256, originalFrames
sampleRate, channels, appliedShiftSamples, canonicalSha256
decode: native decode report
resample: comparison resampling report
originMatch: native sample-window correlation report
scope: measured decoder correction, no musical/Flash/device estimate
```

`normalization.json` uses `ddi-danoni-native-normalization/1`, producer
`native-normalization/1`, and contains:

```text
request: NativeWorkRequest
sourceInputs: {"namespace:path":{binding:Binding,path:"evidence/inputs/<hash>"},...}
normalizedManifestSha256
charts: [{id,captures:{projection,motion,audio},worker:{workerSha256,sandboxSha256},
          preparedIteration,sourceMp3Sha256,canonicalSha256,nativePcm,
          presentationDiagnostics:{motion:[],audio:[]},
          optionalColorDiagnostics:{projection:[],motion:[],audio:[]},
          logicalClockAgreement:true,independentDecoderOrigin:audioReport}]
artifacts: [{path,sha256,bytes},...]     all files except this receipt itself
scope
```

The receipt binds normalized manifest and native-scroll-review bytes. The review
keeps `ddi-reviewed-native-scroll/experimental-1`, with
`sourceManifestSha256` and every chart's measured base arrow heights per second,
relative motion path/hash and scope. It can feed `WorkRequest.reviews.motion`
without rewriting absolute paths. A host can provide an `AssetSource` over the
returned artifacts to call `audio::prepare_media`, musical/review stages and
`work::prepare_work`, then install verified prepared songs through its transaction.

## Native job and CLI

```text
{schema:"ddi-danoni-native-work-job/1",roots:{namespace:externalDirectory,...},
 request:NativeWorkRequest}
```

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- normalize-native /tmp/native-work-job.json /tmp/work-new
```

Root paths are absolute or relative to the job. The native adapter rejects Git
checkout inputs/outputs, symlinks and containment escapes. Output must be fresh;
its parent must exist. Artifact namespaces, complete receipt inventory, bytes and
manifest identity are checked before writing. `manifest.json` is renamed last;
canceled/failed owned partial outputs are removed. Existing trees are never
replaced. CLI output is a small count/hash/scope receipt, not the complete chart
JSON. It is not yet a URL-to-library command.

## Local acceptance and remaining proof gaps

Final results:
`~/Music/DanOni/reviews/rust-normalization-20261013/corpus-verified/`.
All **78 works / 113 requested charts** match every existing normalized chart
field other than expected music-container hashes and the previously documented
stronger-contract additions. Those additions are explicitly enumerated per chart.
There are **67,988 taps / 3,637 holds / 75,262 endpoints**. Canonical audio has
identical sample dimensions, maximum PCM difference **1**, and unchanged measured
origin corrections. **18 chart joins across all 16 affected works** pass the
exact decoration proof.

The trial feeds each returned manifest/scroll review/WAV to the Rust neutral
media and whole-song producers using an in-memory `AssetSource`, then reads the
resulting **78 songs / 113 charts** with the real game SSC parser. Every tap/head/
tail agrees with exported endpoint identity/time; maximum source-grid difference
is `2.8421709430404007e-11` ms (floating-point noise). All **2,679** authored scroll
segment checks preserve measured base speed and author multipliers at x1/x2/x3.
This is an exact-source fallback-grid smoke test, **not acceptance of 75 BPM as
musical tempo**. Native musical fitting remains available separately and
provisional under [milestone 3](danoni-rust-tempo.md).

Three deliberately isolated selectors reject before any partial output:

- RE:RAVE Hard: `native-source-bank-missing`, the existing absent speed-bank proof.
- Destined Marionette Original: `native-motion-segment-unproved`, the existing
  motion segment crossing author speed changes.
- M. MOON `chart-1` (`★ぞろり★`): `native-run-controls-mismatch`. Its projection/audio
  configs include a click at frame 330 `(70,212)` that its motion config omits.
  Current individual motion/clock checks pass, but they do not prove that omitting
  that UI action preserves every selected script setting. Recapture movement with
  the exact selector plan or produce a specific state/bytecode equivalence proof;
  do not guess the click is harmless. The other M. MOON chart remains in the trial.

The two partial works are explicitly selected subsets in local jobs. Their
accepted exports are unchanged. This join does not erase the earlier two gaps,
weaken the event comparison or mark an entire pack freshly reproduced.

```sh
DDI_DANONI_NORMALIZATION_CASES="$HOME/Music/DanOni/reviews/rust-normalization-20261013/cases.json" \
DDI_DANONI_NORMALIZATION_OUTPUT=/tmp/danoni-normalized-new \
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test normalized_corpus -- --nocapture
```

Cases are local-only `ddi-danoni-normalization-parity-cases/1`: root,
expected works/chart/rejection counts, successful `{id,request,baseline:Binding}`
rows and explicit `{id,request,code}` rejected selectors. Output is optional and
fresh. Six additional synthetic tests cover exact decoration multiplicity,
control-plan identity, configured resource/movie bindings, PCM dimensions,
rejected/canceled requests, readiness, partial-write cleanup and changed inventory. Acquisition and fresh execution never
run during these gates.

Next: port the separately pinned Higan/Kouhaku chart contracts, resolve or recapture
these three proof gaps, then package/port the restricted worker supervisor and
resource-request loop. Source discovery/URL fetching and desktop installation
remain the subsequent host stages. The current saved-run API removes the Python
normalization/demux/audio dependency for the proved structural corpus.
