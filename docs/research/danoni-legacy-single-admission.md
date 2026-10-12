# Legacy single-song admission after gen0

Implemented 2026-10-12. Seven cached single-song works received restricted native
title/menu/play probes. Three provide **13 admitted five-key charts, 10,748 taps
and 666 holds**. All source bytes, recordings, captures and generated artwork
remain under `~/Music/DanOni/`; no third-party fixture is committed.

| Work | Admitted selections | Taps / holds | Selected published meters |
| --- | --- | --- | --- |
| [闇夜の月](http://varenntainn.oboroduki.com/yamiyo.html) | Ten author charts | 9,519 / 563 | レク 9; MFV2 11; eight unrated |
| [歪んだ神](http://shomotsu.g2.xrea.com/danoni/sec1/kami2.html) | Hard | 633 / 43 | 12 |
| [What's Going On](http://blossom130.g3.xrea.com/danoni/doxf04/wgo.html) | Easy, Hard | 596 / 60 | 5, 6 |

## Native contracts and exact timing

Yamiyo fits the existing additive, per-arrow `dac-5` contract. Its selected
`keyLabel` is the number **5**, rather than the string `"5"`. The adapter now
accepts that explicit integer representation and still rejects fractional,
negative, boolean and unsupported modes. The title option temporarily retains
an earlier pixel increment; the actual Play initialization supplies
`set_spi == 2 * speed == olds`, so movement uses the gameplay state.

The ten admitted author labels, in native selector order, are レク, ＫＫＫＫＫ,
MFV2, 疾風, max, tickle, Darkfox, ratilt, 食麺麭慢 and YNQ. Every selector
independently supplies unconsumed banks, default movement and the exact MP3
onset: native movie frame **160**, worker iteration 502 for the saved recipe.
Every measured first-arrow arrival agrees exactly with the raw frame. YNQ's
initial authored value 0.25 gives 0.5 source pixels/frame; its measured first
arrow moves at 30 px/s while the baseline remains 2 px/frame. The additive law,
150-frame spawn lookahead and SSC global-scroll approximation remain as in the
[gen0 contract](danoni-gen0-admission.md#per-arrow-speed-is-different-from-global-scroll).

The other three charts independently fit `scheduled-counter`. Compiled spawn
plus `judgeY` matches every native head; hold pairs and key definitions remain
intact. Their `frame_num` advances once per iteration; audio begins at native
counter **201**, worker iteration 559. Each measured first-arrow arrival differs
by one native tick, within the existing 1.5-frame observation bound. The reference
records that render phase instead of shifting source timestamps.

All three recordings pass native PCM versus FFmpeg comparison with zero measured
decoder-origin shift. Minimum accepted correlation is 0.99509. Yamiyo uses
22.05 kHz stereo; the other two use 44.1 kHz stereo. Every shared recording is
matched independently for each admitted selector. Canonical WAV and quality-5
Ogg retain the recording's original rate/channels. Original Flash/device latency
remains unmeasured.

## Comment tails are stored data, not executable events

The Yamiyo dos file places author comments after several final commas. Native
`split(',')` keeps those comments in `speed_dac`; a strict all-numeric read used
to reject otherwise valid charts. The adapter now checks the **exact selected
loaded field against the complete native split array**, then separates only a
terminal sequence of empty/comment-only strings after complete event pairs.

A terminal `//...` time token converts to NaN and cannot match the controller's
movie-frame equality. An all-empty time token denotes zero, whose application
frame is −150, before play initialization. Neither tail supplies an executable
speed event. Strings inside a pair or inside the numeric prefix still fail;
missing multipliers and unrelated trailing tokens are not repaired. Note and
hold queues retain their existing strict checks.

`projection.inactiveSpeedTail` adds an optional list of original native string
objects (`type`, readable `text`, complete `utf16`, `truncated: false`). It is
also retained on the normalized chart; `effects` adds an
`inactive-native-speed-tail` entry with those values. No synthetic frame-zero
change is exported, and original bytes/state remain available. The two added
synthetic tests check numeric key labels and both accepted/rejected tail shapes;
the full Python suite has 96 tests.

## Ratings preserve the publisher's scale

The cached [レク catalogue](https://cw7.sakura.ne.jp/lst/ds0065/) lists Yamiyo's
レク selection at 9; the [MFV2 catalogue](https://cw7.sakura.ne.jp/lst/ds0301/)
lists MFV2 at 11. The remaining authors have no confirmed numerical rating in
the cached source pages. Their names remain labels, with Edit/0 for the portable
fallback. Author participation counts are not a source rating or a reliable
note-count oracle; for example, the work page's MFV2 total is arithmetically
inconsistent, and its KKKKK arrow count excludes the onigiri lane.

The [歪んだ神 catalogue](https://cw7.sakura.ne.jp/lst/ds0080/) gives Hard/12,
while its original page shows three filled stars. What's Going On's original
page prints `★×469` and `★×563`, while its
[catalogue](https://cw7.sakura.ne.jp/lst/ds0010/) gives 5 and 6. Each rating review
keeps both original notations, extracted numerical values, exact excerpts,
retrieval provenance and separate scale/authority labels. This batch selects
the catalogue scale consistently with the earlier corpus. It does not average
scales, assume the large numbers are note counts, or infer ratings from names.
The generic meter-to-tier classifier is unchanged.

Each work receives a plain source-title/artist banner. Musical grid fitting,
independent phases on shared music, pickup previews, Vorbis quality and native
base-scroll compensation retain the existing generic policies. No work-specific
fitting parameters are introduced.

## Pending and excluded selections

- **Yamiyo / Fly-Sky:** selected native onigiri data contains `11_data=` inside
  its queue, and several hold banks contain two empty tokens. This is outside
  the terminal speed-comment contract; the tool does not guess a repair. Its
  successful diagnostic run remains saved, but no complete reference is admitted.
- **歪んだ神 / Death:** native conversion hits the unchanged 20-second parent
  CPU limit before producing a complete snapshot. Its run receipt is preserved
  as a pending selection. Raising a per-script setting to the already allowed
  10 seconds did not supply a complete result; source code is not altered.
- **What's Going On / 11kNormal:** the native menu explicitly selects mode 11.
  It is deliberately excluded from the supported 5/7/7i/9A/9B scope, rather than
  represented as a nine-key chart.
- **[NIGHTMARE](http://tseirproodni.sakura.ne.jp/danoni/1-100/2-6/5/nightmare.html):**
  the initial raw-looking bank already mixes strings and numbers, with some
  directional heads shifted 37 frames relative to the loaded fields. The later
  constant schedule therefore needs an explicit source-transformation contract;
  weakening raw/source agreement would hide the customization. Both selectors
  remain pending despite their successful native runs.
- **[Luvin' You](http://clock.ame-zaiku.com/luvin.html):** both selected converters
  halt with the native script-execution diagnostic, even with the existing
  10-second per-script setting. Halts are retained and rejected.
- **2005 [Air](http://www7a.biglobe.ne.jp/~tyruster/rank/air/) and
  [Cold Breath](http://www7a.biglobe.ne.jp/~tyruster/rank/cold/):** ordinary play
  loads their cached dos, exposes child `*_timeline` arrays and a child `Cnt`
  counter, and starts one observed 22.05 kHz **mono** stream with declared
  `latencySeek = 1633`. These are stronger leads than the static survey's many
  embedded stream heads, but child-clock and nonzero-seek audio proof are still
  required. Ranking endpoints are not fetched. They are the next native batch.

The two partly admitted works retain explicit `pendingSelections` in their
recipes/registry entries. The unsupported 11-key selection is recorded separately
as `excludedSelections`. Original acquisition manifests remain unchanged.

## Local layout

- Original acquisitions: `~/Music/DanOni/single-song-corpus-v2/<work>/`.
- Native inputs, title/menu/proof captures, recipes and static action reviews:
  `~/Music/DanOni/native-adapters-v8/<work>/`.
- Immutable normalized outputs: `~/Music/DanOni/normalized/<work>/`.
- Source ratings, artwork, triage, inventory and validation:
  `~/Music/DanOni/reviews/corpus-v2-legacy8-20261012/`.

`recipe-final.json` binds the unchanged acquisition and every admitted selector's
restricted-run/config/PCM hashes. `proof-<index>-v1` supplies final observations.
`native-triage.json` records every attempted run, including failures; static
inspection reads actions as data and never executes acquired work scripts on
the host. Use the existing [extraction commands](danoni-native-adapters.md#running-it)
and [offline rebuild procedure](danoni-exports.md). `native-adapters-v8` is
protected from staging cleanup alongside the earlier evidence directories.

## Accepted generation and validation

The promoted `~/Music/DanOni/exports/` generation contains **91 songs / 139
charts, 94,378 taps and 4,520 holds**. Single Songs has **86 songs / 133 charts**,
with 98 selected published meters and 35 unrated charts. All 264 prior playable
files and the entire Kouhaku ZIP are byte-identical. The registry has 87 required
and 49 pending source entries; three admitted works retain pending supported
selectors. The original v2 inventory is now 69 accepted, three partly accepted
and 41 pending sources.

All 87 required entries pass the real game-parser verifier. New chart endpoints
have maximum error **8.98440 ms**; the 139-chart global maximum remains
**8.99839 ms**. The 96 Python tests and `just check`, including REUSE, pass.
Fresh Chrome imports both collections without errors/warnings, verifies counts
and tiers, and decodes all 91 banners. Twenty-second autoplay checks produce
159 W1/eight holds for Yamiyo's YNQ, 111 W1/three holds for 歪んだ神, and
78 W1/five holds for What's Going On Hard. There are no lower judgments,
failures or let-go results. The generic fitting policies remain unchanged.

Receipts are `validation-summary.json`, `generation-comparison.json` and
`browser/` in the batch's review folder. Generation SHA-256:
`0cb47219af371af9186d7f8ab66eb4d98c67ce26991263913a94c5c08f529328`.
Registry SHA-256:
`f2d5e79c3f4cd55d3be5021f280748578dd81f288d14e552e219f654a726025a`.
The playable ZIP remains
`~/Music/DanOni/exports/single-songs/DanOni Single Songs.zip`.

The next native work is the 2005 child-clock/seek contract for Air and Cold
Breath, followed by the remaining embedded-data engines and runtime-named
carrotcastle resources. Failed selectors keep their diagnostic evidence rather
than weakening the current contracts.
