# Gen3 native corpus admission

Updated: 2026-10-11. Eleven further source works contribute **21 proved charts**:
19 five-key and two seven-key. One additional native selector remains pending.
The expanded local exports contain **44 songs / 72 charts, 52,627 taps and
1,906 holds**. Every song has a banner; 50 of the 66 single-song charts have
published integer ratings. The same BPM fitting, pickup-preview and native-scroll
policies apply to this batch. No song identifier changes an analysis threshold.

## Sources and selections

These are the previously acquired `autumnleaves/` gen3 sources. Original page,
SWF and sidecar receipts remain unchanged. Counts below describe the admitted
native selections, rather than the catalogue's sometimes incomplete chart list.

| Source work | Captured labels / keys | Taps / holds | Selected published meters |
| --- | --- | --- | --- |
| [風神少女 ～ The Crow's Newspaper](http://autumnleaves.yukishigure.com/Crow/aya.html) | Normal / 5 | 656 / 23 | 11 |
| [メイドと血の懐中時計](http://autumnleaves.yukishigure.com/maid/maid.html) | Normal / 5 | 526 / 3 | 9 |
| [月の雫](http://autumnleaves.yukishigure.com/moon/drop.html) | Easy / 5 | 122 / 0 | 1 |
| [Cynthia](http://ikazch.ikaduchi.com/cynthia/cynthia.html) | kAz, レク, mi, ぞろり / 5; tickle, Air / 7 | 3,540 / 123 | レク 6; five unknown integer meters |
| [Sacrifice -断罪-](http://autumnleaves.yukishigure.com/DOXF04/Sacrifice/Sac.html) | Normal / 5 | 401 / 20 | 7 |
| [曽根崎心中](https://sound.jp/kkkkk_1126/doxf04/sonezaki/shinju.html) | Normal / 5 | 940 / 114 | 23 |
| [タイヨウノハナ](http://autumnleaves.yukishigure.com/DOXF04/flower/SUN.html) | クラウド, レク, izanaki, saber / 5 | 2,637 / 215 | 6, 12, 13, 20 |
| [G Free ～ BakuretsuDream](http://autumnleaves.yukishigure.com/DOXF04/G/Free.html) | izanaki, レク, クラウド / 5 | 1,302 / 130 | 13, 7, 6 |
| [River flows "Short Ver."](http://autumnleaves.yukishigure.com/River/flow.html) | Normal / 5 | 531 / 1 | 10 |
| [祭りの中の嫉妬心](http://autumnleaves.yukishigure.com/paru/paru.html) | Normal / 5 | 285 / 8 | 6 |
| [アリス・ゲーム](http://autumnleaves.yukishigure.com/_Alice/Game.html) | Hard / 5 | 710 / 39 | 12 |

The [レク catalogue](https://cw7.sakura.ne.jp/lst/ds0065/) supplies exact linked
work/creator/key-mode rows. タイヨウノハナ and G Free's own pages additionally
publish integer levels for their collaborators, explicitly attributed to
それは秘密's difficulty tool. These are published levels, not newly computed
scores or runtime selector counters. The [shared classifier](danoni-source-ratings.md)
uses their integers unchanged. The catalogue's Cynthia/レク integer 6 is selected;
the original page's six decimal levels (16.19, 6.14, 9.92, 16.95, 7.98, 13.41)
are retained separately. There is no inferred decimal-to-star conversion.
曽根崎心中's page also mentions a tool value 24.43; it is a different published
scale from the selected catalogue integer 23, not an extracted star count.

### Pending selection

タイヨウノハナ has a fifth option, 鉋モドキ, with published level 16. Its AVM1
converter halts at iteration 65 after exceeding the worker's 10-second per-script
budget; other probes also hit the unchanged 20-second process CPU limit. No
complete schedule, motion or music-origin reference is admitted for this option.
Its page, dos, published rating, halted snapshot, audit and failed parent receipts
remain local. Four independently proved sibling charts are exported. This result
does not establish whether the original Flash player could complete the chart.
The recipe and registry record it explicitly in `pendingSelections`.

## Proofs and compatibility changes

The existing `scheduled-counter` family works here. Each selected chart is
captured through ordinary native menu clicks, with Motion off and Practice gauge
where needed. Every tap and hold head is cross-checked against compiled
`mkArrow` / `mkFArrow` plus `judgeY`, before the `frame_num` counter consumes the
data. Holds retain their source tails. Key modes come from the selected native
state: a shared SWF can offer both 5 and 7 even when its catalogue lists only 5.
Numeric `diffin` is a source-bank selector, never a difficulty rating.

Initial speed now additionally requires `set_spi == 2 * dif_spi > 0`, when those
fields are present, and is corroborated by observed arrow movement. This avoids
applying an already active author multiplier twice. Native `speed_dac` values
are authoritative. Crow's `option.txt` supplies 1.3 at frame 485, but its later
`dos.txt` overwrites that field with 1.25; the compiled bank contains 1.25.
This is resource-load precedence, not multiplier quantization. Dumping the
first loaded text alone would miss the overwrite. Other parsed events retain
fractional values such as 0.3 and 1.3.

Movement is measured in one stable author-speed segment. An early linear
extrapolation cannot cross a later speed change during the same arrow's flight;
measure near arrival, or choose another lane whose first arrow has a stable
segment. G Free/レク also demonstrates native stage quantization: a stored
3.5 × 1.25 = 4.375-pixel increment becomes 4.4 pixels per assignment in the
observed positive stage coordinates. The pinned
[Ruffle Twips implementation](https://github.com/ruffle-rs/ruffle/blob/89a7049965f3090e706265670df309a5128233ac/swf/src/types/twips.rs)
uses 20 twips per pixel and truncates pixel conversion. The measurement checks
positions and per-frame increments within one twip (0.05 source-stage pixels),
retains the independently proved baseline, and keeps the existing 1.5-frame
arrival bound. A synthetic corruption test rejects a larger positional error.
The native worker and its resource limits are unchanged.

Music starts at actual `frame_num` 145, 152 or 157 in this batch. Each recipe
binds a snapshot at its own exact stream-start iteration, the native PCM hash,
the source SWF, the immutable input config and the restricted parent receipt.
Independent MP3 demux/FFmpeg decoding must correlate against energetic native
PCM windows before producing canonical audio. A longer capture supplies Alice's
otherwise quiet intro with enough energetic windows. Shared music is admitted
only after separate chart-origin/decoder comparisons agree.

曽根崎心中's sidecar was requested over HTTP and retrieved over HTTPS through
a recorded 301. Its HTTPS movie needs the receipt's actual final sidecar URL;
no scheme alias is guessed and nothing is fetched again. Some native credit
strings are mojibake or `undefined`; preserve them and the original bytes, while
using the cached surrounding page/catalogue for human-readable metadata.

Parsed boosts remain source/reference evidence and explicit export omissions.
Arbitrary backgrounds, gradients, custom fades and other engine presentation
are not reproduced. Native baseline and supported author speed changes become
SSC `#SCROLLS`, multiplied by the player's Xmod; Cmod remains the player's choice.
Plain title banners use the existing renderer and reviewed local artwork policy.

## BPM and coloring follow-up

Default analysis produces a musical BPM for every new song and keeps every
converted endpoint below 9 ms displacement (largest new value: 8.9803 ms).
That timing bound does not prove the chosen beat phase, tempo octave or coloring.
Cynthia's six charts, タイヨウノハナ and G Free currently use the existing audio
fallback when joint chart coherence is weak. Their fine-tick fallback endpoint
counts are 3,336/3,786, 2,332/3,067 and 1,196/1,562 respectively. These preserve
timing but offer weaker musical coloring evidence than the other new works.
Cynthia's catalogue additionally publishes 187 BPM; the automatic audio fallback
selects approximately 186.233. These were the original admission results. The
[subsequent generic refinement](danoni-shared-tempo.md) now compares independent
chart phases and tightly bounded tempo refinements: rhythmic endpoint coverage
rises to 91%, 97% and 95% respectively, without pack-specific thresholds. Original native note frames,
PCM and fit diagnostics remain available for that comparison.

## Local layout and reruns

```text
~/Music/DanOni/
  native-adapters-v4/<id>/
    inputs/                         immutable resources, exact saved configs
    title-*, menu-*, proof-*, final-* native captures and restricted run receipts
    recipe-final.json               accepted chart bindings; pending selections
  normalized/<id>/                  canonical audio, source inputs and references
  reviews/corpus-v2-gen3-20261011/
    <id>/{native-scroll-review.json,ratings/,plain-title-v1/}
    native-prototypes/, presentation-prototypes/  unaccepted attempts preserved
    registry-before.json, registration-candidate.json, admission-inventory.json
    browser/                        import, image and unchanged-file receipts
  exports/                          active local packs
  exports-history/                  prior accepted generations and journals
```

For a fresh normalization, run a final recipe with the existing analysis
requirements and installed FFmpeg. The recipe specifies all exact capture paths,
SHA-256 hashes, selected modes/labels and movement iterations; it is the durable
rerun interface. No downloaded ActionScript runs in this host-side command.

```sh
uv run --with-requirements tools/danoni-dump/ruffle/requirements-analysis.txt \
  python tools/danoni-dump/ruffle/extract_native.py \
  "$HOME/Music/DanOni/native-adapters-v4/reku-061-cynthia-cynthia/recipe-final.json" \
  /private/tmp/cynthia-export-new
```

Re-executing an input config uses the restricted `ruffle/run.py` wrapper and a
fresh output directory. The worker remains network-free; exact resource bodies
are already bound by the input config. See [native contracts](danoni-native-adapters.md)
and [export rebuild/promotion](danoni-exports.md) for the complete workflow.

The registry contains 136 entries: **40 required / 96 pending sources**, plus
the one pending selector within an admitted work. Of the original 113 v2 source
entries, 24 are fully admitted, one is partly admitted and 88 remain pending.
The remaining planned generations are gen1's 25 and gen4's 13 sources; gen0,
other legacy collections, Wayback cases and CW probes remain separate work.

## Validation and next work

The exact staged generation passed the independent game-parser verifier for all
72 charts, including endpoint bounds, held endpoints, source meters, preview
bounds and native movement compensation at x1/x2/x3. Fresh-profile installed
Chrome imported both packs with 44 songs / 72 charts / 54,533 judged heads,
zero errors or import warnings, and decoded every banner across 5, 7, 7i, 9A
and 9B menus. All 99 pre-existing playable files and the entire Kouhaku ZIP
are byte-identical. The verified generation was promoted with archival journals.

The 79 Python tests, 50 Node tests (including real Chrome capture/policy tests),
`just check` and REUSE lint pass. The native worker is unchanged. No third-party
work, music, graphics, dump or export is committed. Receipts bind the browser
import and unchanged-file comparison to the generated index and ZIP hashes.

Next: improve joint tempo/phase fitting for charts sharing music, using this
batch's preserved phases, published BPM evidence and fallback statistics. Then
continue gen1 and the remaining gen4 sources. The halted SUN selector needs a
separate bounded converter investigation, not an exception to admission checks.
