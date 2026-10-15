# Automatic native menu profiles

Implemented 2026-10-14 in `tools/danoni-dump/rust/src/profiles/`, behind
`native`. This extends [explicit selector orchestration](danoni-rust-orchestration.md)
with menu observation and automatic capture planning. It adds no dependency and
makes no change to the game workspace or WASM build. Acquired scripts execute
only in the installed confined Ruffle worker. The API is blocking; call it on a
job thread in a desktop shell.

The [early-menu extension](danoni-rust-early-profiles.md) adds a separately
identified gen1 profile with its own menu frame, controls and required offscreen
renderer. The original later-profile API/defaults below remain available. The
[variable-travel extension](danoni-rust-scheduled-profiles.md) adds a source-bound
five-key gen3 subset using its stored spawn/travel ledger, observed Practice
settings and a separate control contract.

## Scope and identification

The first profile, `reku-constant-menu/1`, recognizes a shared レク engine by five
exact SHA-256 fingerprints of control/action payloads, their tag types, timeline
paths and frames. They bind shared initialization/compilation actions, menu state and timeline
placement; the profile source lists every exact role/hash. These are engine control fingerprints,
not whole-song hashes. The inventory includes nested AVM1 actions/buttons and
AVM2 tags without evaluating or exporting bytecode. All five roles must match
exactly once, the movie must be 60 Hz / SWF version at most 8, and AVM2 action
tags reject identification. The bounded identity inventory now emits requested
control roles plus every AVM2 tag; `fingerprintScope` explicitly distinguishes it
from a full script inventory. Emitted records remain bounded to 16,384.

Identification only proposes controls. Every chart still needs complete runtime
menu observations, loaded-source/compiled-schedule agreement, measured movement,
source-bound matching role controls, independent MP3/PCM origin verification and
full normalization. Unknown engines return `unsupported-native-profile` before
launching a worker or creating output. This is not universal Flash support.

The profile currently admits exact runtime key labels `5` and `7`. It retains
other observed rows and records them in `excludedSelectors` with
`unsupported-key-label`; it never interprets a nine-lane chart as 9A/9B or admits
9i/9t. 9A and 9B remain supported elsewhere in the exporter but need their own
reviewed native menu/control contracts. Readiness covers requested observed supported
selectors, not hidden charts or unsupported rows. Optional `selectors: [0, 2]`
filters zero-based menu indices; omitted means every supported row. Unselected
rows remain explicit exclusions. See the [hold/selector extension](danoni-rust-hold-motion.md)
for exact bounds, hold-head witnesses and source-derived intro inputs.

## Input API

```rust,ignore
profiles::identify(&Movie, checkpoint) -> Result<EngineProfile>
profiles::read_menu(&Value) -> Result<Menu> // original later-profile wrapper
profile.read_menu(&Value) -> Result<Menu>
profiles::selector_controls(&Menu, index) -> Result<SelectorControls> // original wrapper
profile.selector_controls(&Menu, index) -> Result<SelectorControls>
profiles::motion_candidates(&chart, &origin, checkpoint)
    -> Result<Vec<MotionSelection>>
profiles::execute(&ProfileRequest, &WorkerSpec, &AssetSource, new_output, progress)
    -> Result<ProfileOutcome>
profiles::execute_job(installed_bundle, job_path, new_output, progress)
    -> Result<ProfileOutcome>
```

The job schema is `ddi-danoni-native-profile-job/1`:

```json
{
  "schema": "ddi-danoni-native-profile-job/1",
  "roots": {"corpus": "/absolute/local/corpus"},
  "request": {
    "movie": {"namespace":"corpus","path":"work/movie.swf","sha256":"<actual SHA-256>"},
    "sourceManifest": {"namespace":"corpus","path":"work/manifest.json","sha256":"<actual SHA-256>"},
    "context": {"namespace":"corpus","path":"work/context.json","sha256":"<actual SHA-256>"},
    "metadata": {},
    "optionalColor": null
  }
}
```

`context.json` is only original acquisition/runtime input:

```json
{
  "movie": "movie.swf",
  "sourceUrl": "https://example.org/work/song.swf?name=num301",
  "pageUrl": "https://example.org/work/index.html?name=num301",
  "flashVars": {"name":"num301"},
  "resources": [
    {"url":"https://example.org/work/dos.txt","path":"dos.txt","sha256":"<actual SHA-256>","httpCharset":"Shift_JIS"}
  ]
}
```

These examples are structural, not runnable jobs. Movie/resource paths resolve
against the context's containing directory. Source manifest/movie/context and
every resource are immutable hash-bound inputs. The configured movie must also
match the explicit movie binding. Relative namespace roots resolve against the
job file. Unknown fields reject: a context cannot supply selector clicks,
observation frames, renderer options or interpreter budgets. The profile owns
those. Original logical URL/query, page URL, FlashVars, charset and resource bytes
are preserved; staging rewrites only local binding paths.

`optionalColor` uses the existing [exact optional-colour review](danoni-rust-normalization.md)
with bound original 404 bodies, URL/status/date/hash and source movie identity.
The derived review relocates response binding paths only. Each probe applies the
same existing audit review, and final normalization independently checks it again.
A missing file is never replaced with an invented empty file.

## Observations, selectors and units

The reviewed title click is worker iteration 40 at logical stage coordinate
`(160,200)`. Two captures at 45/46 must show root movie frame 3. The menu reads
stored UTF-16 values without getters or string coercion:

- `difficulty` is the number of menu rows, 1–16, not the selected difficulty.
- `diff_label[]` and `key_label[]` must have exactly that length.
- `dif_shift` is the zero-based selected index. `diffi` / `keyLabel` must equal
  that row's stored labels. `diffin` is empty for the first row and numeric
  `index + 1` otherwise.

Repeated observations must agree. Incomplete graphs, holes/duplicate stored
properties, unstable selection, wrong clock/menu frame, halts and disallowed
runtime errors reject. The next-selector control clicks `(40,35)`, starting at
iteration 43 with two iterations between clicks. The number of clicks is derived
from the observed initial selection and row count, including wraparound. Start
clicks `(50,230)` at `max(50, 46 + 2*clicks)`.

The original later profile observes iterations `start-2` through `start+8`, plus 1200 for
stream discovery and a useful PCM overlap. The two pre-start observations must
prove the intended selected row. Exactly one MP3 stream must start after the
start action and by iteration 1200. Its start is discovered, not a fixed 201-frame
recipe. If not already observed, another capture adds that exact iteration,
checks identical stream metadata and reproves the selector. Audio origin uses
the stored `frame_num` at the exact stream-start observation. Worker iterations,
root movie frames, native logical frames and audio sample frames are separate
units; no hard-coded preroll or musical BPM follows from them.

Movement proposals use the first tap or explicitly paired hold head of each
nonempty lane, observed constant travel time and the observed
native-clock/worker-iteration difference. Early hold heads take priority. Three
window positions (half, quarter and three-quarter travel) propose six consecutive
iterations, bounded to 1–1200. The entire window must precede arrival and avoid
speed changes and nonpositive speed. The source lane order supplies clip names
(`le/do/up/ri/oni` or `le/led/do/oni/up/rid/ri`), `_s0` and `_step`; `le_step`
supplies receptor geometry. No per-song note timestamp or target Y is supplied.
Hold clips add the `f` prefix and must be observed before judgment. Each
proposal must pass the unchanged geometry/arrival and clock-agreement proof. Failed trials remain in the receipt; exhausting proposals fails closed.

All profile captures use fixed `inspectionScale:4` and `avmExecutionMs:2000`,
within the existing supervisor maxima; dense chart compilation needs larger
stored graphs. Output, CPU, wall, memory and interpreter confinement remain in
force. Projection/motion/audio keep exactly the same script inputs and selector
controls. Observing a six-frame motion window is not proof by itself.

## Budgets, output and readiness

`request.limits` defaults to 64 captures, 8 motion trials per chart, 180,000 ms
wall time and 2 GiB total output. Maxima are 64 / 12 / 300,000 ms / 2 GiB.
`captureLimits` contains the existing per-worker supervisor limits. Preparation
stages at most 4,096 files / 128 MiB, with at most 64 resources / 64 MiB total,
16 MiB per resource and a 64 KiB context. Each graph/config/capture retains its
existing independent bounds. Output reservations include worker/input copies and
normalization overhead; periodic disk checks and cancellation span the entire
operation. Arbitrary third-party values cannot raise maxima.

```text
profile-run/
  inputs/source-manifest.json       original acquisition provenance
  inputs/original-context.json     byte-identical context
  inputs/blobs/<sha256>             immutable movie/resources/404 evidence
  inputs/<probe>.json               generated exact capture configurations
  probes/<probe>/                   confined supervisor receipts, state and PCM
  motion-<selector>.json            accepted measured geometry/clock proof
  normalization-request.json        full existing NativeWorkRequest
  replay-job.json                   portable explicit NativePlan using this tree
  normalized/                      standard normalized manifest/evidence/media
  manifest.json                    parent success, written last
  failure.json                     retained failure instead of success marker
```

The parent receipt schema is `ddi-danoni-native-profile-run/1`, producer
`native-menu-profiles/1`. It records `ready`, `profile`, original `request`, trusted
`worker`, `fingerprints`, `fingerprintScope`, original `sourceInputs`, observed `menu` and its bound
capture, `excludedSelectors`, all child capture receipt hashes, every
`motionTrials` candidate/result, `playInputs`, `failedSelector`,
`pendingDependencies`, generated `plannedCharts`,
`normalized` paths/hashes/chart count and `failure`. Generated chart IDs are
`chart-<zero-based-menu-index>`; original runtime difficulty text stays in chart
metadata. This does not classify free-text difficulty as a numeric rating.

The parent becomes ready only after every requested observed supported row normalizes and
a final cancellation/output check passes. Late cancellation removes the pending
marker and clears the parent's normalized result even if child files exist.
Failures retain captures; child readiness alone is not installation authority.
No accepted pack, export registry or library is changed.

## Run and validation

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- profile-native \
  /absolute/trusted/worker-bundle /tmp/profile-job.json /tmp/new-profile-output
```

Run offline with previously acquired public inputs. No network request occurs.
A required missing GET produces `native-profile-needs-dependencies`, retaining
structured exact request evidence. If required GETs accompany the optional-colour
failure, the exact colour review is deferred and no audit entry is suppressed;
`pendingDependencies[].optionalColorReviewDeferred` records this. Pending requests
are evidence, not authorization to refetch an already reviewed 404. [Dependency recovery](danoni-rust-dependencies.md)
also supports [automatic pre-menu profile recovery](danoni-rust-profile-recovery.md)
through `execute_with_dependencies` and `profile-native-dependencies`. The lower
level command described here still requires a populated source context and does
not fetch missing inputs. Neither entry point is URL-only importing. Host robots/terms/rate-limit/cache
approval, index discovery and library installation remain later milestones.

Original tests cover fingerprints, UTF-16 menus, unsupported key labels, invalid
or inconsistent state, selector wraparound, sparse lanes, clock offsets, speed
boundaries, cancellation and unknown-engine rejection. Set
`DDI_DANONI_WORKER_BUNDLE` and `DDI_DANONI_PROFILE_JOB` for installed-worker tests
of complete selection, portable replay preparation, late cancellation and limits.
`DDI_DANONI_PROFILE_CASES` enables the separate local frozen-corpus/PCM/game-parser
gate; `DDI_DANONI_PROFILE_RECEIPT` writes its result to a fresh external file.
Third-party jobs/captures/results remain under `~/Music/DanOni/`.

### Initial local result, 2026-10-14

The same profile matches eight acquired works. Seven normalize successfully,
covering eight charts. No per-work selector/motion recipe is supplied.

| Local work ID | Observed admitted charts | Coverage |
| --- | --- | --- |
| `reku-045-noname08-noname` | 5-key Remake / Hard | Selector enumeration, suffix change, holds, speed changes, retained presentation data |
| `reku-047-7keyfestival-march` | 7-key Normal | Holds, exact missing-colour 404 review |
| `reku-048-7keyfestival-gkbr-riguru` | 7-key Normal | Holds, optional-colour review |
| `reku-050-dosmf08-last-saigo` | 5-key Normal | Three speed events, optional-colour review |
| `reku-051-dosmf08-60s-wind-wind` | 5-key Hard | Dense chart, holds, six speed events |
| `reku-052-dosmf08-regret-enemy` | 5-key Hard | Dense compilation, ten speed events |
| `reku-18b-7keyfestival-syoujosyuu-danmaku` | 7-key Normal | 109 holds, five speed events, optional-colour review |

The local parity gate compares every gameplay/source-data field against the
independent frozen normalized corpus, mapping old chart IDs through the stored
native selector index. IDs/reference paths, independently reproved capture
iterations, WAV encoding bytes, relocated optional-colour paths and the new
structured GET diagnostic are explicitly accounted provenance differences.
Retained presentation/effect values and HTTP evidence still compare. It passes
5,179 taps, 128 holds, 5,435 parsed endpoints and 102 x1/x2/x3 scroll checks.
Maximum measured parser error is `2.8422e-11` ms; canonical PCM differs by at most
one signed 16-bit unit. These results do not approve new musical tempo fits.
The seven successful operations used 32 captures and nine motion trials.

The initial `reku-053-dosmf08-dariscradle-slashspeed-maspa` trial exposes three
5-key menu rows but returns `native-profile-motion-unproved`. Its no-input run reaches root label
`goResult` with `life = 0` / stopped logical counter 195 before the first tap
witness. The original explicit corpus recipe supplies play keys across early
holds. All eight automatic motion trials fail; no parent success or normalized
work is published. Retain this as a separate failed source, not three admitted
charts. A generic follow-up needs hold-motion observation or bounded derived
play inputs shared by **every** projection/motion/audio run and full source
reproof. Do not copy that song's manual key schedule into this profile.

The subsequent [hold-head/derived-input slice](danoni-rust-hold-motion.md) closes
that motion/audio gap for requested rows 0 and 2. Row 1 still rejects its
pre-compilation source rewrite; it has not been admitted. Those fresh trials and
reference comparisons live in a separate dated directory.

All jobs, original failed probes, successful captures and comparison results live
in `~/Music/DanOni/reviews/rust-profiles-20261014/`. `cases.json` binds the local
frozen baselines and rejection; `parity-result.json` records the downstream check.
Accepted `exports/index.json` and `export-registry.json` remain unchanged. The
native/pure suites, both Clippy configurations, formatting and `just check`
(including REUSE) are required independently of this optional local gate.
