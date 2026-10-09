# Native AVM1 inspection prototype

Experimental native Ruffle host for the [legacy extraction plan](../../../docs/research/danoni-legacy-plan.md).
It captures stored ActionScript data and optional offscreen pixels. It is **not
yet a general DanOni SWF-to-dos converter**. Export adapters cover the original
synthetic fixture, Higan's default five-key Normal and the pinned
[2004 Kouhaku collection](../../../docs/research/danoni-kouhaku.md) (five songs, six charts).
The [native handover](../../../docs/research/danoni-native.md) records its
cross-checked arrays, music origin, export contract and remaining limits.
Existing CW dump `/1` files and schemas are unchanged.

## Build separately from the game

Tested on macOS with Metal, Rust 1.98.0 and Java 26 (`java` must be on PATH for
Ruffle's playerglobal build). Python 3.10+ runs the preparation and restricted
launcher. No Python packages are required. Browser comparison additionally uses
the parent tool's `npm ci` dependencies and installed Chrome.

Ruffle is pinned to release **v0.7.1**, revision
`89a7049965f3090e706265670df309a5128233ac` (release checked 2026-10-08).
The first debug build compiled 329 packages in roughly five minutes on this
machine; later worker-only builds took seconds. Build outside all Git checkouts:

```sh
mkdir -p /private/tmp/ddi-ruffle
git clone --depth 1 --branch v0.7.1 --filter=blob:none --sparse \
  https://github.com/ruffle-rs/ruffle.git /private/tmp/ddi-ruffle/upstream
git -C /private/tmp/ddi-ruffle/upstream sparse-checkout set core render swf flv wstr video tools/asc
python3 tools/danoni-dump/ruffle/prepare.py \
  /private/tmp/ddi-ruffle/upstream /private/tmp/ddi-ruffle/build
cargo build --manifest-path /private/tmp/ddi-ruffle/build/Cargo.toml --locked
cargo clippy --manifest-path /private/tmp/ddi-ruffle/build/Cargo.toml --locked --no-deps -- -D warnings
```

`prepare.py` verifies the revision and tracked changes, adds three small hooks
(inspection module export, stored-property access, halted-VM access), and copies
our inspection modules and host into the scratch build. It never executes a
movie. `Cargo.lock` pins host dependencies independently of the game's lockfile.
The capture embeds `build-identity.json`: upstream revision plus SHA-256 of the
preparer, manifest template, lockfile and our Rust sources. The parent runner
records the actual worker binary hash. Upstream source is not vendored here;
retain its MIT/Apache-2.0 licences in the checkout. Our additions and original
fixture generator are MIT, covered by the repository's `REUSE.toml` annotation.

## Run and validate the original fixtures

Always use `run.py`, never execute an acquired SWF with the worker directly.
Outputs and synthetic/generated media must remain outside Git too.

```sh
python3 tools/danoni-dump/ruffle/test_native.py \
  /private/tmp/ddi-ruffle/build/target/debug/ddi-ruffle-probe \
  --directory /private/tmp/ddi-ruffle/check-1
python3 tools/danoni-dump/ruffle/fixture.py /private/tmp/ddi-ruffle/integer-input-1 --integer
python3 tools/danoni-dump/ruffle/run.py \
  /private/tmp/ddi-ruffle/build/target/debug/ddi-ruffle-probe \
  /private/tmp/ddi-ruffle/integer-input-1/run.json /private/tmp/ddi-ruffle/integer-run-1
python3 tools/danoni-dump/ruffle/export_fixture.py \
  /private/tmp/ddi-ruffle/integer-run-1/snapshot.json /private/tmp/ddi-ruffle/integer-export-1
node tools/danoni-dump/ruffle/cw-roundtrip.mjs \
  "$HOME/Music/DanOni/corpus/jam-with-pancake" \
  /private/tmp/ddi-ruffle/integer-export-1 /private/tmp/ddi-ruffle/integer-cw-1
```

The last command requires the existing local Jam with Pancake CW engine dump;
it does not download it. It verifies cached resource hashes, substitutes only
our synthetic chart/silence and serves every browser request from local bytes.
Unexpected requests and WebSockets are denied. Read `conversion.json` for the
engine identity, counts, timing discrepancies and maximum error in seconds.
Exit 0 means this comparison passed; exit 2 means it failed. The tested v48.4.1
engine preserves the integer fixture exactly. Omitting `--integer` produces a
240.5-frame note: CW rounds it to 241, and the comparison **must fail** with
`maxErrorSeconds` approximately `0.5 / 60`. This is a negative test, not an
accepted lossy export. The synthetic mode validates notes/holds only. Higan's
mode also compares speed and expanded arrow colours and subtracts CW's observed
automatic lead-in when comparing audio-relative times.

Run `python3 tools/danoni-dump/ruffle/test_projection.py` for original synthetic
projection/demux tests, including empty long-form SWF blocks, unknown seek
patterns, incorrect sample counts and damaged initialization evidence.

Native integration checks cover exact arrays before/after runtime mutation,
holds, shared/circular references, sparse arrays, undefined/nonfinite/-0 values,
Japanese strings, getter non-execution, URL query and FlashVars, frame-polled
Enter, child-movie-clip data, exact offscreen RGB pixels, delayed stereo PCM
impulses/sample counts, repeatability, cache integrity, exhausted
snapshot/audit limits, VM halt rejection, output location, filesystem/network
denials, and parent time/memory termination. No third-party fixture is committed.

## Input config

Acquisition remains a separate step using the existing robots-aware, paced
cache. Create a dedicated input directory containing only the work's allowed
resources and config. Keep original response bytes, provenance, retrieval dates,
credits, HTTP charset and hashes there. The entire input directory is readable
by the worker, so do not use a home directory or a shared scratch root as input.

```json
{
  "movie": "blobs/SWF_SHA256",
  "sourceUrl": "https://example.invalid/work/movie.swf",
  "pageUrl": "https://example.invalid/work/index.html",
  "flashVars": { "difficulty": "Hard" },
  "captureFrames": [1, 30, 60],
  "render": true,
  "mixAudio": false,
  "events": [{ "frame": 40, "type": "enter" }],
  "resources": [{
    "url": "https://example.invalid/work/dos.txt",
    "path": "blobs/DOS_SHA256",
    "sha256": "64-lowercase-hex-digits",
    "httpCharset": null
  }]
}
```

`movie` and resource paths are relative to the config directory; the SHA strings
above are placeholders. `sourceUrl` must be anonymous HTTP(S). Preserve its query
and the original page URL/FlashVars; absent HTTP charset stays null, rather than
inventing a Shift_JIS response header. Only exact cached GETs without a request
body are served. Missing requests are recorded; nothing fetches them automatically.
AVM2 and LZMA/ZWS inputs are outside this pilot.

`captureFrames` selects 1–16 worker iterations, each from 1 through 600. These
numbers are **not** the movie's timeline frame or a verified song clock.
`events` accepts up to 64 actions: `enter`, `key`, or `click` with numeric `x`/`y` in
the fixed 320×240 viewport. Press occurs before the selected iteration; release
occurs before the next. Capture beyond the release iteration when inspecting
post-input state. `key` takes `code` (Enter, ArrowLeft/Right/Up/Down or Space)
and optional `holdFrames` (1–120; default 1). Releases must fit within the
iteration limit. `inspectGlobals` defaults to false; true records the two AVM1
case-sensitivity environments' enumerable stored global properties, excluding
function values and non-enumerable properties. `render` defaults to false; true uses an offscreen Metal target.
The upstream bundled default font is enabled, including with rendering disabled.
Its metrics matter for autosized text button hit testing. `mixAudio` defaults to
false; true installs an offline stereo 44.1 kHz Ruffle mixer, advancing by the
cumulative sample count for each header-FPS iteration. No audio device is opened.

## Output contract (experimental)

| File | Contents |
|---|---|
| `run.json` | Parent report, schema `ddi-ruffle-run/experimental-1`: UTC start, exit code, termination reason, elapsed seconds, peak RSS KiB, limits, binary/config/sandbox hashes and `inputsUnchanged` for those three inputs. |
| `snapshot.json` | Worker result, schema `ddi-ruffle-probe/experimental-1`: Ruffle/build identity, SWF hash, original config, SWF header FPS, clock limitations, `avm1Halted`, `rendered`, `captures`, `audit`. May be absent, empty or partial after termination; check `run.json` first. |
| `stderr.txt` | Launcher/worker diagnostics. Runtime warnings/errors also enter the bounded audit. |
| `frame-<iteration>.ppm` | Optional P6 RGB image, 320×240; no alpha. One per requested capture iteration. |
| `audio.pcm` | With `mixAudio`: raw stereo interleaved signed 16-bit little-endian PCM at 44.1 kHz, starting at worker iteration 1; includes menu/lead-in silence. The snapshot's `audio` report identifies successful stream starts by iteration and exact output sample-frame cursor. |

Each `captures[]` entry is `{frame: integer, state: Snapshot}`. A `Snapshot`
has schema `ddi-avm1-snapshot/experimental-1`, `roots`, `objects`, `displayTree`, `globalScopes`, `limits`,
`excluded`, `incomplete`, and `avm1Halted`:

- `roots[]`: `{level: integer, movieFrame: integer|null, value: Value}`.
- `objects[]`: `{id: integer, kind: string, properties: Property[], truncated: boolean}`.
  IDs are zero-based indices local to one snapshot; never compare IDs between
  unrelated captures. Kinds are `object`, `array`, `movie-clip`, `function`,
  `native-unsupported`. Native internal state is not inspected.
- `displayTree[]`: bounded native display traversal, including child MovieClips;
  `{id,parent,kind,name,depth,visible,movieFrame,stageBoundsPixels,object}`.
  Parent is a display index or null, name is StringValue or null, object is a
  graph reference or null. Bounds `[xmin,ymin,xmax,ymax]` use original stage
  pixels, before viewport scaling/letterboxing. Kinds additionally include
  `avm1-button`, `edit-text`, `text`, `display`. Child storage is inspected
  directly; script MovieClipReference resolution remains excluded.
- `globalScopes[]`: empty unless requested; otherwise
  `{caseSensitive:boolean,properties:[{name:StringValue,value:Value,accessor:boolean}],excluded:string[]}`.
  Stored enumerable data only; global function values are explicit uninspected
  tags. It uses the same graph/string/property budgets and never runs getters.
- `Property`: `{name: StringValue, value: Value, enumerable: boolean, accessor: boolean}`.
  Only stored own properties are traversed, including non-enumerable properties.
  Arrays keep their stored `length` and index properties: an absent index is a
  hole, distinct from an explicitly stored undefined value.
- `Value`: JSON null/boolean/finite number, `{ref: integer}`, StringValue,
  `{type:"undefined"}`, `{type:"number", value:"NaN"|"Infinity"|"-Infinity"|"-0"}`,
  or `{type:"uninspected", reason: string}`.
- `StringValue`: `{type:"string", text:string, utf16:integer[], truncated:boolean}`.
  `utf16` is authoritative (including unpaired surrogates); `text` is a lossy
  display preview. Reasons include `prototype`, `accessor`,
  `movie-clip-reference`, and `object-limit`. Inspection never invokes getters,
  methods or ActionScript coercion.

`incomplete` means an inspection budget was exhausted, not that all relevant VM
state was otherwise observed. Bounds: 64 roots, 256 displays, 1,024 objects, 20,000 properties,
4,096 UTF-16 units per string and 262,144 total units per snapshot. Exclusions
are always explicit. `audit` has `entries:[{kind,detail,truncated}]`,
`droppedEntries`, `truncatedDetails`, `runtimeErrors`, `truncated`; bounds are
256 entries and 1,024 characters per detail. Do not silently accept lost evidence.

With mixing enabled, `audio` is `{sampleRate:44100,channels:2,sampleFrames:integer,
streamStarts:StreamStart[],scope:string}`; otherwise it is null. StreamStart is
`{iteration,outputSampleFrame,sliceStart,sliceEnd,compression,sampleRate,stereo,
samplesPerMovieFrame,latencySeek}`. Positions count stereo sample frames;
`sliceStart`/`sliceEnd` are diagnostic Ruffle movie-data byte offsets, not original
SWF-file offsets. A start records successful decoder initialization, not perceived
audible onset. The offline mixer does not verify arbitrary stream seeks.

The synthetic exporter writes `charts/fixture/{chart.dos.txt,chart.html,keys.json}`,
`media/synthetic.wav`, and `manifest.json` (`ddi-synthetic-export/experimental-1`).
It uses one five-lane chart with audio-relative seconds in the manifest and CW
frames in dos. HTML is a data carrier, not a player. It explicitly labels the
result reconstructed and unverified until comparison. It rejects truncated or
halted evidence. Its generated silence is **only** test media, never a substitute
for a real song's embedded music. These files do not stabilize a legacy importer
contract; the agreed multi-chart/custom-key export remains in the work plan.

## Limits and next gate

The macOS runner denies live networking, arbitrary file reads/writes, child
processes and unapproved execution, with no unsandboxed fallback. It depends on
Apple's installed `dyld-support.sb` and grants system/Metal service access; the
passing probes establish the tested restrictions, not comprehensive hardening
against native emulator vulnerabilities. Other operating systems need their
own reviewed launcher before executing acquired movies. Limits include 30 s
wall time, 20 s CPU, 2 GiB monitored RSS, 40 MiB per output file, 128 descriptors,
16 MiB SWF input, 64 MiB declared uncompressed SWF and 64 MiB cached resources.

Successful process completion proves a capture was written. It does not prove
valid chart initialization, successful sound playback or complete playability.
The host calls `run_frame` then advances timers by a header-FPS step, while
`getTimer` still reads wall time. The default `NullAudioBackend` reports sound
position zero; the optional mixer advances a sample cursor. This is not a
general coherent virtual Flash/audio clock. `clock.timebaseVerified`
is therefore always false. The fixture's independently known chart clock is
60 Hz while its SWF header is deliberately 30 Hz. Real-song adapters must
establish their timebase and audio origin before export. The bundled font is a
fallback, not a complete recreation of the author's device fonts; rendering may
still differ. Ruffle's audio feature uses its upstream-constrained Symphonia
0.5.5; the standalone lockfile pins those transitive dependencies.

Higan now has complete pre-consumption arrays, a zero-latency continuous MP3
demux, repeatable observed music origin and a passing CW comparison. Its original
Flash latency and full visual/gameplay fidelity remain unmeasured. The 2004 Kouhaku adapter below adds a second family with explicit nonzero
latency limits. Next cover the known Straight Soul Flash/HTML5 pair. Chart Reverser can supply
an additional authoring-grid comparison where first-number/BPM changes are
known; it cannot independently recover an unknown timebase or original project.

## Pinned multi-song extraction

```sh
python3 tools/danoni-dump/ruffle/kouhaku.py \
  "$HOME/Music/DanOni/2004kouhaku.swf" /private/tmp/kouhaku-export-new \
  --worker /private/tmp/ddi-ruffle/build/target/debug/ddi-ruffle-probe
node tools/danoni-dump/ruffle/cw-roundtrip.mjs \
  "$HOME/Music/DanOni/corpus/jam-with-pancake" \
  /private/tmp/kouhaku-export-new /private/tmp/kouhaku-cw-new tougenkyo
python3 tools/danoni-dump/ruffle/test_kouhaku.py
```

The six-chart extractor needs installed FFmpeg. It preserves embedded MP3 and
uses untrimmed WAV for playback, generates CW dos and `danoni-5` simfiles, and
puts a five-song import pack under `playable/`. Its nonzero SWF audio latency is
explicitly unverified; comparison uses the observed Ruffle nominal frame origin.
The optional final comparison argument chooses a chart id in multi-chart exports.
See [the collection handover](../../../docs/research/danoni-kouhaku.md) for exact
counts, schemas, hidden-chart controls, sample-delay limits and validation scope.

## Ratings, compressed packs and musical-grid analysis

Kouhaku adapter `/2` extracts `difficultyStars` as an integer and keeps
`rawDifficulty` only as provenance; `difficultyClassification` is null.
Simfile meter carries the count, with empty description and the required Edit
placeholder. Consumers own classification and sorting. Existing `/1` outputs
are preserved. `prepare_pack.py` shares rating, preview, audio and SSC policies across normalized
exports. `repack_kouhaku.py` remains a compatibility entry point. Neither executes
the SWF or changes source note times.

`analyze_tempo.py` is optional and uses `requirements-analysis.txt` independently
of the game/native build. It combines chart-grid fitting with librosa 1.0.0
audio hypotheses and local windows; it never automatically applies an inferred
BPM or phase. The synthetic 75 BPM clock remains authoritative.

See [the beat-grid handover](../../../docs/research/danoni-beat-grid.md) for the
exact numeric-rating contract, measured candidate tempos, limits, Vorbis helper
build/repack commands and Chrome sample-alignment checks. The newest Ogg pack
is local at `~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-generic-pack/playable.zip`.
The older `stars-ogg-v2` variant retains its original schema and Edit tiers.


## Generic post-processing

```sh
python3 tools/danoni-dump/ruffle/prepare_pack.py SOURCE_EXPORT /private/tmp/pack-new \
  --audio ogg --vorbis-encoder /private/tmp/encode-vorbis \
  --tier-policy stars --star-tier-bounds 1,3,5,7 --preview-seconds 12
python3 tools/danoni-dump/ruffle/test_pack_policy.py
python3 tools/danoni-dump/ruffle/test_vorbis.py /private/tmp/encode-vorbis /opt/homebrew/bin/ffmpeg
```

Use `--audio wav` without the Vorbis helper; installed FFmpeg is required for
both. Outputs must be fresh and outside Git. Optional musical-grid analysis
uses the same normalized inputs and accepts repeatable `--preferred-bpm ID=BPM`
review hints. No collection IDs, source SWF hashes or musical BPM overrides are
built into these policies. Native source adapters still establish engine-specific
chart/audio origins before producing those inputs. HTML5 browser dumps need a
normalization adapter; their existing reference JSON is not this input format.

[Generic contract and limits](../../../docs/research/danoni-postprocess.md)
describes normalized input, SSC/manifest/ZIP output, tier bounds, previews,
unsupported effects and cross-family validation. MIT coverage comes from the
repository's existing REUSE catch-all; no work assets are fixtures.


`prepare_pack.py --display-bpm CHART_ID=200` (repeatable) adds reviewed standard
`#DISPLAYBPM` metadata while keeping `#BPMS:0=75` and all note times unchanged.
Ranges and explicit unknown are supported. The initial generic pack had no
such tags. See the [display-BPM handover](../../../docs/research/danoni-postprocess.md#display-bpm-handover-exporter-and-menu-are-separate):
the current game summary also needs to honor explicit display BPM for SSC
imports; adding the tag alone will still show 75 until that game-side fix.


`prepare_pack.py --musical-grid REVIEW.json` is an opt-in **approximate** musical
SSC export. It repositions notes and timing together under a strict below-10 ms
budget, snaps compatible notes to reviewed subdivisions and falls back to fine
ticks for outliers. Defaults remain exact. Every endpoint shift and fallback is
reported; invalid/collapsed holds or conflicting cells reject the export. See
[the review schema and measured limits](../../../docs/research/danoni-postprocess.md#opt-in-musical-timing-with-a-strict-error-budget).
`test_musical_grid.py` covers the independent synthetic cases. The newest local
musical variant is `~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-musical-grid-v3/playable.zip`.


The musical variant changes beat-based X-mod scroll velocity as well as note
quantization. It does **not** preserve original DanOni vertical speed. See
[the independent-motion handover](../../../docs/research/danoni-postprocess.md#scroll-speed-must-stay-independent-of-musical-colouring)
for the maintainer's clarified target and the difference between compensating an
old SSC baseline and verifying the actual native engine's motion.


## Measured native base scroll

`measure_motion.py` reads stored display bounds from restricted capture outputs;
caller-selected clip paths, source geometry and verified FPS produce a measured
arrow-size velocity. It performs no native/script execution or fetching. Then
`prepare_pack.py --native-scroll REVIEW.json --scroll-factor 1` emits authored
`#SCROLLS` compensation beneath the user's unchanged X-mod. It can be combined
with the reviewed musical-grid option; C-mod remains the player's override.

[Review schema, measurement limits and complete commands](../../../docs/research/danoni-postprocess.md#measured-native-scroll-with-a-tunable-correction)
include six fresh native probes, measured source units and engine comparisons.
`test_scroll_policy.py` covers original synthetic measurements, per-chart base
compensation, author changes, evidence binding and adjustable factors.
The newest local trial is
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-musical-native-scroll-v3/playable.zip`.
Use `--scroll-factor 1.2` or `0.8` when regenerating a fresh pack for a 20% faster
or slower authored baseline; BPM, audio, notes, colours and user settings remain
unchanged. Exact source exports and earlier experimental ZIPs are preserved.
