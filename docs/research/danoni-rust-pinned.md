# Rust pinned Higan and Kouhaku adapters

Implemented 2026-10-13 within [milestone 4](danoni-rust-plan.md#4-native-extraction-and-supervision).
The existing `prepare_projection` and `prepare_native_work` APIs now cover the
separately reviewed [Higan](danoni-native.md) and [2004 Kouhaku](danoni-kouhaku.md)
contracts. These are exact-movie adapters alongside the seven structural
families. No work name, selector or pinned setting enters the generic BPM,
preview, codec or scroll-fitting algorithms.

The production Rust APIs read bound saved state and media; they do not launch
Ruffle or need Python, FFmpeg, Node or a browser. This trial used the existing
MIT Python supervisor to take fresh **restricted development captures**, then
Rust performed the entire passive normalization and downstream preparation.
Porting and packaging worker supervision remains open.

## Selection and source identities

Use the existing [projection](danoni-rust-extraction.md) and
[normalization](danoni-rust-normalization.md) job schemas. `ProjectionOptions`
adds `pinnedSelection: string | null`, default null. Allowed additions are:

```json
{"family":"higan-pilot","mode":"5","pinnedSelection":null}
```

```json
{"family":"kouhaku-2004","mode":"5","pinnedSelection":"tougenkyo"}
```

Kouhaku selection values are `tougenkyo`, `tougenkyo-hard`, `run`, `kitakaze`,
`korokke` and `emy`. A Kouhaku selection is required; selections on other families
reject. Both pinned families reject prefixes, suffixes, decoration exceptions
and other key modes. These contracts support the reviewed default settings;
they do not discover arbitrary selector trees or new engine customizations.

| Contract | Exact original SWF SHA256 |
| --- | --- |
| Higan | `6ce53b126e90b25a9f307595bf226ed4e6ce8f70c74d816a1d372c7f20de6e85` |
| Kouhaku | `e14e472911b6603ddcc58bd40049d0b05cfa986a2240209dd848a516cf40fed2` |

Actual movie bytes and all saved capture/run/config/resource/PCM bindings pass
normalization's existing hash, successful-run and cross-run control checks.
Both movies require 60 Hz. No relaxed movie alias, omitted selector event,
changed source configuration or blanket runtime-error allowance is introduced.

## Chart proof and reference representation

The Rust contracts port our MIT [Higan projector](../../tools/danoni-dump/ruffle/higan.py)
and [Kouhaku projector](../../tools/danoni-dump/ruffle/kouhaku.py), using stored
UTF-16 strings and complete graph arrays. Acquired ActionScript is never parsed
as host code or executed in the host.

Higan proves ordered split/prepared/cleanup stages, normal five-key selection,
Motion OFF, zero correction, the 100-frame spawn lead, native keys, every tap
and hold transformation, duration caches and complete counted tap/hold
schedules in both prepared and cleanup states. Unused diagonal lanes are not
playable lanes. The sole MP3 start must occur at logical `frame_num = 145`, with
`music_act` on frame 2 and zero declared latency. Music comes from
`root/sprite1602` and must satisfy the stricter zero-latency continuity check.

Higan speed frames remain native judgment frames. Its native color frames are
spawn-side values: normalized seconds are `(colorFrame - 100 - 145) / 60`.
`al` expands to all five lanes and `oni` to the space lane. Both raw triplets and
expanded colors remain in the reference. Unreviewed stop/color targets reject.

Kouhaku proves the selected title/artist, original filled-star string, hidden
chart state and default `Reverse=false`, `SpeedUp=2`, `SpeedY=188`. Every source
CSV bank must match its complete initialized timeline before consumption, with
`Cnt=1`. The controller's parent clip must be on frame 2 at the sole music start
and advance to frames 3 and 4 in the next two captured iterations. The stream
must be 22,050 Hz mono MP3, declared latency 1,633 samples, on the selected
reviewed sprite route. Payload continuity and zero declared latency are separate
facts: Kouhaku does not satisfy the latter. The declared seek is retained rather
than guessed away.

Kouhaku counts the actual checked runtime `★` string. Normalization emits
numeric `difficultyStars` (3, 8, 6, 5, 4, 5), retaining `rawDifficulty`; it does
not classify a tier. Existing complete-song preparation applies the generic
star classifier. Higan's selector index `difficulty=1` is **not** a star count
and remains unrated here; its separate source-bound published-rating review can
supply a rating. Per-selection title/artist override work-wide fallback metadata
so collections preserve distinct song credits. Shared MP3/canonical WAV identity
still groups Tougenkyo's two charts into one song.

The normalized layout and readiness/inventory receipts stay unchanged. Each
chart adds:

```text
pinnedReference: complete ddi-danoni-avm1-reference/experimental-1 object
  adapter: reku-2.16-higan/1 | iso50-kouhaku-2004/2
  sourceSwfSha256, ruffle, fps, stages, audioOrigin, metadata, lanes
  speed, boost, colors, original contract limits and agreement fields
clockField: "frame_num" | "pinned-child-timeline"
initialPixelSpeed: observed Higan increment | 2
pinnedClockPath: string[]                 Kouhaku only
```

`pinnedReference` retains the full original adapter representation, including
Higan keys and native color events. Its lane keys are `judgementFrames`,
`holdFrames`, `tapsSeconds`, `holdsSeconds`, plus spawn-frame evidence. The shared
normalized chart uses `nativeFrames`, `nativeHoldFrames` and audio-relative
seconds as before. All source frames, hold endpoints and effects agree with the
frozen references. No original dos text is invented: these SWFs use embedded or
compiled state, and the existing portable normalized/SSC output is a derived
representation.

## Movement and audio clocks

Pinned motion uses `pinned-bounds-width/1`, explicitly measuring the translating
bounds and **reference width** used by the original pinned reviews. Higan uses
`le_s<digits>` and `le_step`, left lane only. Kouhaku uses
`<lane>MC<digits>`, the corresponding `<lane>_step` receptor, and `left_step` as
the common geometry reference. In particular, the onigiri receptor's wider box
must not change the chart's reference scale. `geometryReference` may be omitted
for this pinned default; an explicitly different reference rejects.

Select at least three strictly increasing captured `iterations`, a supported
lane, and no synthetic `targetY`. Selected visible movie clips must be unique.
Local display IDs and parent indices are checked; cycles, changed hierarchy,
changing arrow shape/reference/receptor bounds, horizontal drift, nonlinear
translation and changed movement settings reject. Their logical counter must
advance exactly once per worker iteration and explain the initialized pixel
increment within 0.05 pixels. Higan's witness must precede nonunit author speed
changes.

Kouhaku's root movie remains on its selected scene while the child song clip
advances. Its `pinnedClockPath` is derived from the unique initialized controller's
parent ancestry, not a remembered display-array index. Audio and motion carry
identical `clockPath` arrays and `clock="pinned-child-timeline"`; the same logical
counter-minus-worker-iteration offset must hold for every motion sample. Higan
uses `frame_num` in both roles.

The existing motion schema additionally records:

```text
policy: "pinned-bounds-width/1"
clockPath: string[]                       Kouhaku only
arrivalProved: false
arrivalDifferenceFrames, rawFirstFrame, observedArrivalMovieFrame
referencePixels, pixelsPerSecond, baseArrowHeightsPerSecond
samples: [{iteration,movieFrame,x,y,targetY},...]
scope: translation/width proof; nominal frame timing, no centre-arrival claim
```

Measured bases reproduce **180 / 45.95 = 3.917301414581067** for Higan and
**120 / 43 = 2.7906976744186047** arrow heights/second for every Kouhaku chart.
The generic structural adapter's arrival test is unchanged. Pinned bounding-box
arrival is diagnostic: Higan's top arrives one frame after its raw first frame;
Kouhaku top/centre geometry differs from nominal frames. This does not justify
moving notes or claiming original Flash hit-box timing. Original pinned limits
are retained in the combined reference rather than replaced with a stronger
arrival claim.

The sole observed stream is demuxed unchanged and independently correlated with
each chart's bound native PCM. All seven measured decoder corrections are zero.
Kouhaku's nonzero Flash header seek remains explicit; correlation proves current
Ruffle decoder alignment, not original Flash/device latency. No musical phase
or inferred BPM enters this correction.

## Local acceptance

Results remain outside Git:
`~/Music/DanOni/reviews/rust-pinned-20261013/`.

- `captures-v1/<selection>/inputs/` contains dedicated immutable original blobs,
  byte-identical acquisition provenance and the capture config.
- `captures-v1/<selection>/native/` contains snapshot, parent sandbox receipt,
  20-second mixer PCM and a current Python differential reference.
- `kouhaku-inputs-v1/` contains identical hashed movie/config copies for the
  explicitly shared movie binding used by the six-chart normalization request.
- `cases-v1.json`, `jobs-v1/` declare source-bound jobs and differential witnesses.
- `corpus-verified/` contains final Rust normalized works and the downstream
  acceptance manifest. `cli-verified/` contains byte-compared CLI normalization
  outputs and failure/readiness results.

All seven fresh captures used the existing deny-by-default macOS sandbox with
no network access. The already built worker SHA256 is
`0cf02071e7fa0bc610b6188c8db48df8cc75c913cc67a1900ca21f7fbd8f5c73`, Ruffle
revision `89a7049965f3090e706265670df309a5128233ac`. Higan's earlier capture had
only about 83 ms of post-start audio, insufficient for the origin policy;
20-second runs provide enough nonquiet comparison windows. No source files were
fetched again. Original acquisition dates, including Kouhaku's unknown date,
remain as recorded rather than becoming recapture dates.

Acceptance covers **2 works / 6 songs / 7 charts**, **3,055 taps / 9 holds /
3,073 endpoints**. Every full current pinned reference matches, including units,
metadata, clocks and limits; frozen note/effect fields also match. Independent
WAV decodes have identical rate/channels/frame counts and maximum signed PCM
difference **1**. Every SSC endpoint passes the real game parser; maximum error
is `2.8421709430404007e-11` ms. All **57** authored-segment checks preserve base
scroll and user x1/x2/x3 multiplication. Kouhaku's two shared-audio charts retain
independent origin proofs and deduplicate to five songs. Numeric stars and
generic difficulty tiers also survive the SSC parser; Higan remains unrated.

This downstream smoke uses the exact-source fallback grid, not acceptance of
75 BPM as musical tempo. The generic native tempo/phase review remains open;
accepted exports and their reviewed musical grids are unchanged. The earlier
78-work/113-chart structural gate also passes after adding the pinned path.

```sh
DDI_DANONI_NORMALIZATION_CASES="$HOME/Music/DanOni/reviews/rust-pinned-20261013/cases-v1.json" \
DDI_DANONI_NORMALIZATION_OUTPUT=/tmp/danoni-pinned-new \
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test normalized_corpus -- --nocapture
```

Pinned case rows use `pinnedReferences` instead of a legacy normalized `baseline`:
`[{id,binding:Binding,frozenReference:Binding,decodedMusic:Binding,
expectedShiftSamples:integer,expectedBaseArrowHeightsPerSecond:number}]`.
The gate compares complete current references, frozen note/effect data,
audio-relative lanes, actual stars, independently decoded PCM and measured width
before the shared downstream media/SSC/scroll checks. Original synthetic tests
cover stage/order/settings/selection failures, source/native disagreement,
schedule loss, child clocks/ancestry, accessors, source hashes, cancellation,
changed motion shapes/increments/references and wrong audio sprite routes.

Next: package and port the restricted worker supervisor and versioned capture
protocol. Three structural selector proof gaps remain as listed in the
[normalization handover](danoni-rust-normalization.md#local-acceptance-and-remaining-proof-gaps).
Acquisition, resource-request iteration, recipe-free URL inputs, HTML5 control
and desktop library installation remain later milestones.
