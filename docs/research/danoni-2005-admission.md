# 2005 Air and Cold Breath admission

Implemented 2026-10-12. Two original SWF6/AVM1 works now supply **five verified
five-key charts and 2,041 taps**, without holds or authored speed/colour queues.
Movies, dos, audio, screenshots, ratings evidence and exports stay local under
`~/Music/DanOni/`; no third-party fixture or music is committed.

| Work | Native selector | Loaded suffix | Taps | Selected catalogue meter | Original filled stars |
| --- | --- | --- | --- | --- | --- |
| [Air](http://www7a.biglobe.ne.jp/~tyruster/rank/air/) | Normal | empty | 614 | 8 | 4 |
| [Cold Breath](http://www7a.biglobe.ne.jp/~tyruster/rank/cold/) | Easy | empty | 125 | 1 | 1 |
| Same work | Standard | `2` | 279 | 4 | 4 |
| Same work | Hard | `3` | 530 | 7 | 3 |
| Same work | Special | `4` | 493 | 11 | 2 |

The publisher's [catalogue](https://cw7.sakura.ne.jp/lst/ds0002/) dates these
works to April/May 2005. Catalogue **Normal** corresponds to Cold Breath's
native/page **Standard**. That explicit reviewed mapping is kept with the
rating evidence; it is not a generic free-text difficulty classifier. Both
published scales are retained, with the catalogue scale selected consistently
with the existing corpus. The unchanged tier classifier yields Challenge for
Air and Beginner/Medium/Hard/Challenge for Cold Breath.

## Clock and selected input

The earlier child-clock suspicion was incorrect. Both movies store split
`left/down/up/right/space_timeline` arrays in a child controller, but schedule
spawns against the **parent movie frame** at `rawHead − 150`. The child `Cnt`
increments on arrow duplication and wraps at 500; it counts arrow instances.
During play the root movie timeline advances once per native worker iteration.
Six first-arrow observations per selector agree **exactly** with the raw note
frame and measure 2 source pixels/frame, 120 px/s, against a 43-pixel directional
receptor. Baseline scroll is therefore approximately **2.79070 arrow heights/s**.
The independently captured song onset is root movie frame **190** in every
selector. Exact note seconds remain `(rawHead − 190) / 60`.

Cold Breath's selector is not an own property of the captured root. The existing
`child-timeline-5` adapter now accepts optional recipe **`sourceSuffix`** rather
than substituting a guessed clock or reading a guessed object id:

```json
{
  "id": "chart-1",
  "family": "child-timeline-5",
  "mode": "5",
  "label": "Standard",
  "sourceSuffix": "2"
}
```

This is a fragment of the existing
[hash-bound recipe schema](danoni-native-adapters.md#running-it); projection,
motion and audio capture bindings remain required. The suffix is a string of
up to 32 letters/digits/underscores, with the empty string allowed. The adapter
requires every selected loaded lane field and checks it against all native
prepared frames in repeated unconsumed snapshots. Both historical suffix
placements are supported; ambiguity, root-selector conflicts, missing/mismatched
lanes, selected holds/effects it cannot represent, and source speed without a
native speed bank fail. The exact reference adds optional
`projection.selectedSourceSuffix`; the clock remains `"root movie frame"`.
Native key-code arrays are absent and remain null, without inferring custom keys.

Ordinary native menu clicks independently select all four Cold Breath charts.
The title/menu/proof runs use only previously cached `dos.txt`; the acquisition
page and source bytes remain immutable. Ranking/BBS/CGI endpoints are untouched.
Static actions are disassembled as data on the host; acquired scripts execute
only inside the restricted native Ruffle worker with unchanged parent guards.

## Music and presentation limits

Both works start exactly one observed **22.05 kHz mono MP3** song stream with
`latencySeek = 1633`. Runtime slice boundaries identify the actual stream among
the movies' many static embedded stream heads. The demux report keeps that seek
value, every block header, exact payload hashes and the original MP3.

Independent native-PCM versus decoded-payload comparisons for all five selectors
find **zero decoder-origin shift and zero lag spread**. Minimum accepted
correlation is **0.995828**. No speculative 1633-sample trim is applied. This
result belongs to these recordings and the pinned Ruffle runtime; other streams
still need their own origin measurements. Canonical WAV and quality-5 Vorbis
keep the original rate/channel count. Original Flash/device latency is unmeasured.

Each work has a plain source-title/SHIKI banner. The generic musical-grid fitter,
pickup preview policy and native-base-scroll compensation remain unchanged.
Air fits approximately 177.5325 BPM and Cold Breath approximately 199.400 BPM,
close to published 178/200. Different Cold Breath charts share the recording
while retaining independent fitted phases. No work-specific fitting parameter,
manual BPM override or note-time correction is introduced.

Special's native menu selects a **Stair** field with gap 4. Its raw note frames
and measured default movement are retained; the portable five-key SSC presents
ordinary receptors and does not reproduce that field geometry or arbitrary
original visual effects. Matching timing/base speed is not a claim of identical
source presentation.

## Local artifacts and regeneration

- Acquisitions: `~/Music/DanOni/single-song-corpus-v2/tyruster-{air,cold}/`.
- Native inputs, selected-menu/proof captures, static review and hash-bound
  `recipe-final.json`: `~/Music/DanOni/native-adapters-v9/<work>/`.
- Immutable references, source files, complete MP3/PCM and per-selector audio
  comparison evidence: `~/Music/DanOni/normalized/<work>/`.
- Ratings, artwork, triage, inventory and validation receipts:
  `~/Music/DanOni/reviews/corpus-v2-legacy9-20261012/`.

The native evidence directory is protected from staging cleanup, including
nested paths. Original acquisition records are not rewritten. Re-extraction uses
`extract_native.py` and the existing offline
[build/promote commands](danoni-exports.md); native extraction does not require
Playwright. Chrome is used separately to validate the resulting game imports.

## Accepted export generation

Current exports contain **93 songs / 144 charts, 96,419 taps and 4,520 holds**
(100,939 judged heads). Kouhaku's five songs/six charts are unchanged. Single
Songs contains **88 songs / 138 charts**, with 103 selected published numerical
ratings and 35 explicitly unrated charts. All 93 songs have banners. The
registry has 89 required and 47 pending source entries. The original v2 inventory
has 71 accepted, three partly accepted and 39 pending sources; pending selector
coverage remains separate from source status.

All 89 required entries pass the real game-parser verifier. New endpoints have
maximum error **8.80158 ms**; the corpus-wide maximum remains **8.99839 ms**.
Every one of the **273 previous playable files** is byte-identical, and Kouhaku's
ZIP SHA-256 remains
`e46cbdf6cec188e0fe98a3ddbf3f05bdbc9013dc416baa76bb53dd572c6a231c`.

Fresh Chrome imports 93 songs/144 charts without errors or warnings, verifies
the numerical tiers and decodes all 93 banners. Twenty-second autoplay supplies
54 W1 for Air, 16 W1 for Cold Breath Easy and 53 W1 for Special, with no lower
judgments, failure or let-go results. These runs used the complete preliminary
ZIPs while final producer bindings were rebuilt; **both final ZIPs match those
tested bytes exactly**. `validation-summary.json` binds the tested artifact
hashes and generation separately from final provenance. This avoids treating
similar source data or equivalent charts as a browser verification of new bytes.
The 97 Python tests and `just check`, including REUSE, pass. Regression coverage
also preserves the older `n`-prefixed note variant's global `speed_data` check.

Receipts are `validation-summary.json`, `generation-comparison.json`,
`native-triage.json`, `admission-inventory.json` and `browser/` under this batch's
review folder. Preliminary normalization/review/generation artifacts are
preserved there or under the native study; they are not accepted exports.
Final generation SHA-256:
`460eef81068212321b1ba7720e4536a748786a241698a16fc0483d8dd8b5fbe3`.
Registry SHA-256:
`5be30b3e0512dddfada45370581db0320efc09707878841307915452ce56650e`.
The playable archive is
`~/Music/DanOni/exports/single-songs/DanOni Single Songs.zip`.
