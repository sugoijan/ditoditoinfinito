# Gen1 native corpus admission

Implemented 2026-10-12. The 25 cached レク generation-1 works expose **31
5-key charts, 18,109 taps and 1,290 holds**, with 246 parsed speed events.
Both [ゆかりんファンタジア](http://autumnleaves.yukishigure.com/Team_9/yukarin_side/Phantasia.html)
and [ばか☆すた](http://autumnleaves.yukishigure.com/Team_9/9_side/bakasuta.html)
offer four independent author charts. Each selector has its own schedule,
movement and music-origin proof; the listing is insufficient to enumerate them.
All third-party sources, captures, media, artwork and exports remain local.

## Constant-travel initialization

The static survey suggested the generic scheduled-counter adapter. Native runs
instead show an older **constant-travel** contract, without `judgeY`. The
existing `scheduled-constant` family now supports both observed initialization
orders; no movie hash allowlist or pack-specific parameters were added.

With the recorded Play click at iteration 50, these works split source strings
at 52, build numeric arrays and sparse schedules at 53, delete their lane arrays
at 54, and initialize `frame_num = 0` at 56. Those iteration numbers describe
the saved input recipe; the adapter recognizes states rather than fixed numbers.
It verifies all of the following before returning a reference:

- Selected loaded tap/hold fields agree with the original split lane banks.
- Compiled tap and hold heads equal raw heads minus `set_cnt`; hold tails stay
  unshifted, and `frzLength` matches every raw duration.
- The schedules, key codes, initial speed and parsed speed bank survive intact
  until an observed zero-counter state, before gameplay consumes them.
- Deleted credits such as `tuning` remain in the earlier reference. Retained
  metadata cannot change between compilation and counter initialization.

Some older AVM1 arrays expose `constructor` alongside the later
`__constructor__` name. Both are metadata, while unknown nonnumeric schedule
properties still fail. The compiled schedule must account for every chart head.
All captured builds run at 60 FPS and use the five-key native lane indices
`[0, 2, 4, 6, 3]`.

The reference schema remains `ddi-native-family-reference/experimental-1`.
Its `projection` adds `compiledIteration` (worker iteration) and, for delayed
initialization, `clockInitializationIteration`. `splitIteration` identifies the
raw source capture; `preparedIteration` is the verified zero-counter witness.
Note frames, hold endpoints and parsed speed-event frames use the independent
native clock. Worker iterations are evidence positions, not song timestamps.
The existing recipe schema and chart IDs (`chart-0` etc.) are unchanged.

## Native timing and presentation

Observed stream origins are native frames 145, 147, 157, 167, 169 or 185.
Seconds are `(nativeFrame - observedMusicOrigin) / 60`; each selection is checked
independently even when its music hashes match another chart. The demuxed MP3
is compared with native PCM before canonical WAV and final Ogg conversion.
Original Flash/device latency remains unmeasured.

Base movement ranges from 2 to 4.5 pixels/frame. Six rendered samples per chart
prove the arrow/receptor geometry, active author multiplier and source arrival.
Observed arrival differences range from -0.099 to +1.000 native frames, within
the unchanged 1.5-frame bound. Parsed `speed_dac` is authoritative: this engine
subtracts `set_cnt` from loaded speed-event frames. For example, Road to you's
loaded `200,0.5` becomes native `50,0.5` with 150 frames of travel. Neither
conversion nor fitting should subtract that travel again.

Mahou and the original five-key Danmaku use a different visible title/Play
layout; Syameimaru also requires its own title target. Ordinary clicks handle
those layouts. Misplaced exploratory clicks produced denied credit-link
navigation, retained in their failed probes; admitted captures contain none.
Owen and Last Judgement require rendering for `BitmapData.draw`. Rendering is
enabled in their final captures, with no error suppression or raised worker
budgets. The Ruffle worker and pinned revision are unchanged.

Automatic BPM fitting, independent shared-song phases, pickup previews, Ogg
quality and inverse native `#SCROLLS` use the existing generic policies. Source
frame references remain exact and fitted SSC endpoints keep the 9 ms bound.
Custom colours, fades, lyrics and other engine presentation retain the existing
explicit omissions; native screenshots and state stay available as evidence.

## Titles, credits and ratings

Titles use the agreed runtime `musicTitle` across each work's selections;
creator suffixes from the catalogue are not added to song titles. Credits prefer
an explicit Artist label in the cached original work page and otherwise retain
the catalogue hint. The local metadata review preserves both, including
MeltyBlood's original 中嶋 spelling and the collaborator credit on ゆかりん.
Japanese pages containing Windows extensions decode as CP932, matching the
browser's Shift_JIS label; original bytes remain untouched. Banners use the
existing plain-title renderer.

Here `difficulty = 4` means four charts and `difStep = 5,5,5,5` means their key
modes. Neither is a star count. Exact linked catalogue rows and matching native
labels provide **23 published integer ratings**, classified with the existing
Kouhaku thresholds without rescaling the publisher's numbers. Six collaborator
charts lack individual published ratings. Two catalogue labels disagree with
the selected SWF: History lists Normal/0 but runs Hard; East lists Normal/11 but
runs Hard. Those eight charts keep the unrated Edit/0 fallback, with their
original names preserved. No rating is inferred from density, label or speed.

| Work | Native selections / published meters | Taps / holds | Speed events |
| --- | --- | --- | --- |
| [終焉と始まり](http://autumnleaves.yukishigure.com/hazimari/hazimari.html) | Normal 4 | 272 / 0 | 6 |
| [恋色ファイナルマスタースパーク](http://varenntainn.oboroduki.com/DO/koiiro.html) | Normal 9 | 484 / 20 | 7 |
| [魔法少女達の百年祭](http://autumnleaves.yukishigure.com/mahou/mahou.html) | Hard 15 | 679 / 50 | 4 |
| [U.N.オーエンは彼女なのか？](http://autumnleaves.yukishigure.com/owen/owen.html) | Hard 21 | 1339 / 68 | 11 |
| [千年幻想郷～History of the Moon](http://autumnleaves.yukishigure.com/history/history.html) | Hard unrated | 934 / 58 | 10 |
| [明治17年の上海アリス](http://autumnleaves.yukishigure.com/alice/alice.html) | Hard 16 | 631 / 23 | 0 |
| [東の国の眠らない夜](http://autumnleaves.yukishigure.com/DOXF03/higasinokuni/higasi.html) | Hard unrated | 620 / 76 | 51 |
| [Phantasm Brigade - another side](http://autumnleaves.yukishigure.com/DOXF03/Phantasm/Brigade.html) | Hard 14 | 765 / 152 | 9 |
| [無何有浄化 ～ Pure in Asia](http://autumnleaves.yukishigure.com/DOXF03/PureIn/asia.html) | Hard 13 | 583 / 61 | 2 |
| [最後の審判](http://autumnleaves.yukishigure.com/DOXF03/Last/Judgment.html) | Normal 10 | 698 / 91 | 26 |
| [恋色マスタースパーク](http://autumnleaves.yukishigure.com/DOXF03/master/spark.html) | Hard 14 | 853 / 57 | 13 |
| [The end of 1000 years](http://autumnleaves.yukishigure.com/DOXF03/TheEndof1000/years.html) | Hard 14 | 616 / 21 | 2 |
| [MeltyBlood](http://autumnleaves.yukishigure.com/DOXF03/MeltyBlood/Melty.html) | Normal 7 | 334 / 31 | 0 |
| [ところが私には通用しない](http://autumnleaves.yukishigure.com/DOXF03/voyage/voyage.html) | Voyage 12 | 419 / 44 | 3 |
| [六十年罪業華地獄残酷歌](http://autumnleaves.yukishigure.com/DOXF03/Zankokuka/Yamada.html) | Normal 10 | 393 / 26 | 2 |
| [童祭　～Snow Festival](http://autumnleaves.yukishigure.com/DOXF03/Snow/Festival.html) | Normal 7 | 435 / 12 | 13 |
| [ゆかりんファンタジア](http://autumnleaves.yukishigure.com/Team_9/yukarin_side/Phantasia.html) | クラウド unrated; レク 8; 暗狐 unrated; 鉋モドキ unrated | 1683 / 202 | 20 |
| [ばか☆すた](http://autumnleaves.yukishigure.com/Team_9/9_side/bakasuta.html) | クラウド unrated; レク 12; 暗狐 unrated; 鉋モドキ unrated | 1504 / 52 | 6 |
| [もう歌しか聴こえない](http://autumnleaves.yukishigure.com/NoSound/mistia.html) | Normal 6 | 256 / 8 | 0 |
| [ナイト・オブ・ナイツ](http://autumnleaves.yukishigure.com/Night/knights.html) | Hard 14 | 651 / 1 | 5 |
| [人形裁判　～ 人の形弄びし少女](http://autumnleaves.yukishigure.com/Dolls/Dolls.html) | Hard 12 | 759 / 47 | 0 |
| [sweet little sister](http://autumnleaves.yukishigure.com/SLS/SLS.html) | Frenzy 30 | 1694 / 58 | 35 |
| [Road to you](http://autumnleaves.yukishigure.com/Road/Road.html) | Normal 7 | 305 / 0 | 6 |
| [妖怪の山](http://autumnleaves.yukishigure.com/Syameimaru/Youkai.html) | Hard 12 | 524 / 17 | 15 |
| [弾・幕・決・壊](http://varenntainn.oboroduki.com/danmaku/danmaku.html) | Hard 13 | 678 / 115 | 0 |

## Local evidence and reproduction

```text
~/Music/DanOni/
  single-song-corpus-v2/autumnleaves/  immutable acquisitions
  native-adapters-v5/<work>/          read-only worker inputs and capture runs
  normalized/<work>/                 source, recipe, references and canonical audio
  reviews/corpus-v2-gen1-20261012/    metadata/art/ratings, inventory and validation
  exports/                          current accepted packs
  exports-history/                  earlier generations and promotion journals
```

Each study work keeps `recipe-final.json`; rerun it into a fresh directory:

```sh
NUMBA_CACHE_DIR=/tmp/ddi-ruffle/numba-cache uv run --offline \
  --cache-dir /tmp/ddi-ruffle/uv-cache \
  --with-requirements tools/danoni-dump/ruffle/requirements-analysis.txt \
  python tools/danoni-dump/ruffle/extract_native.py \
  "$HOME/Music/DanOni/native-adapters-v5/reku-031-doxf03-meltyblood-meltyblood/recipe-final.json" \
  /tmp/meltyblood-export-new
```

Native reruns use the restricted `ruffle/run.py` wrapper with a fresh output;
exact immutable GET bodies are already supplied by the saved input config.
See [native contracts](danoni-native-adapters.md) and
[export rebuilding](danoni-exports.md) for complete commands and units.

## Validation and remaining work

Validation receipts and the generation comparison are local under the review
folder above. The expanded generation is checked by the game's real parsers,
independent audio comparisons and fresh-profile Chrome import/autoplay.
The native scroll verifier now samples inside the first active scroll segment:
Road to you's first author event and baseline both land at SSC beat zero, so
sampling before that point sees the engine's unauthored pre-roll ratio. An
original Rust regression tests both coincident and later author changes at
x1/x2/x3. This corrects verification without changing generated simfiles or
source motion. The failed earlier probe is archived separately.

All 12 previously admitted constant-family charts reproject identically with
the extended adapter. The 90 Python tests, 50 Node/browser tests and the new
Rust verifier regression pass; `just check` and REUSE lint also pass.

The promoted generation contains **69 songs / 103 charts, 70,736 taps and
3,196 holds**. Fresh-profile Chrome imports both ZIPs with zero errors or
warnings and decodes all 69 banners. Three 20-second autoplay checks cover
MeltyBlood (44 W1 / 9 holds), ゆかりん's 鉋モドキ chart (125 W1 / 1 hold), and
East (38 W1 / 6 holds), with no lower judgments or dropped holds. Each session
reloads and checks the debug clock reset to avoid stale state.

All 132 old playable files and the complete Kouhaku ZIP are byte-identical.
All 207 playable files also match the archived pre-fix verifier experiment.
The maximum fitted note-endpoint movement across all 103 charts is
**8.998395 ms**. The generation index SHA-256 is
`d78c566f7f0308495b1134d8d0d3e755abcc97cb7f904ca81864e889d6a5293c`;
`validation-summary.json` binds the ZIP hashes, browser/parser receipts and
static application bundle. Promotion archives the previous generation with its
journals. Original acquisitions, canonical audio and native references remain
outside Git.

The registry has 136 entries: **65 required / 71 pending sources**, plus the
previously pending SUN selector. Of the original 113 v2 acquisitions, 49 are
fully admitted, one partly admitted and 63 pending. All 25 gen1 works are
admitted. The [gen4 admission](danoni-gen4-admission.md) subsequently adds those 13
remaining listed works. Next is gen0; other collections,
Wayback sources and unreviewed CW engines remain separate admission batches.
