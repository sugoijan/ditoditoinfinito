# Published difficulty ratings in the single-song exports

Updated: 2026-10-11. Numeric source difficulty now reaches SSC through a
source-bound review of cached work pages and catalogue rows, supplementing
ratings already extracted from the runtime. **50 of the 66 single-song charts**
have verified published ratings. They use the same classifier as Kouhaku:

| Source count / chosen range maximum | SSC tier |
| --- | --- |
| 0–1 | Beginner |
| 2–3 | Easy |
| 4–5 | Medium |
| 6–7 | Hard |
| 8 or more | Challenge |

The original count is the SSC meter. This heuristic does not make different
publishers' scales equivalent: a 40-level author's 18 remains 18, rather than
being rescaled or replaced with a guessed difficulty. Unknown ratings remain
null in the report and `Edit` / meter 0 in SSC.

## Original single-song source evidence

The [first v2 admission](danoni-corpus-v2-admission.md) and
[paired-source admission](danoni-paired-ports.md), and
[gen3 admission](danoni-gen3-admission.md) document the additional rows,
including Saigetsu 11 and Earthquake Super Shock / レク[Normal] 25.

All pages below were already cached locally. This change downloads nothing and
executes no acquired script. Exact source bodies, encodings, retrieval dates,
URLs and SHA-256 hashes are retained in the reviews.

| Work / chart | Published rating → exported meter / tier | Source |
| --- | --- | --- |
| Higan / Normal | 9 → 9 / Challenge | [レク's catalogue](https://cw7.sakura.ne.jp/lst/ds0065/), row 1 links to the captured Higan page |
| FTHosyu | ★★★★☆ → 4 / Medium | [Work page](http://eucaly.net/~iso50/onigiri3.html) |
| party 4u | ★★★★☆ → 4 / Medium | [Work page](http://eucaly.net/~iso50/onigiri5.html) |
| felys | Eight filled stars → 8 / Challenge | [Work page](http://eucaly.net/~iso50/10/onigiri10.html) |
| RE:RAVE / LVUP | 1→12 → 12 / Challenge | [Archived work page](https://web.archive.org/web/20190314171455/http://www.geocities.jp/naotopage2/game/re_rave/re_rave.htm); both endpoints retained, maximum chosen for presentation |
| RE:RAVE / Hard | Fourteen filled stars → 14 / Challenge | Same archived work page |
| FAKE800 / Hard | 22 → 22 / Challenge | [MFV2's author catalogue](https://cw7.sakura.ne.jp/lst/ds0301/), row 238 links to the exact work; its older [page](https://mfv2.sakura.ne.jp/do/__Flash/mfv2_005/danoni/DanOni/FAKE800/DO.FAKE800.html) uses ★☆☆ on a different scale, also retained |
| Destined Marionette / Hard | 11 → 11 / Challenge | [レク's catalogue](https://cw7.sakura.ne.jp/lst/ds0065/), row 85 links to the captured page |
| Jam with Pancake / Hard | ☆08 → 8 / Challenge | [Work page](https://chipstar64.sakura.ne.jp/danoni/danoni_JwP.html) |
| Straight Soul / 5-key Normal | 13 → 13 / Challenge | [Work page](https://cw7.sakura.ne.jp/danoni/2009/0169_StraightSoul.html) |
| Straight Soul / 7-key Hard | 18 → 18 / Challenge | Same work page, separate key-mode/label association |
| カナリア / 7i Skip | ★×18, izkdic's 40-level scale → 18 / Challenge | [Work page](https://vorhandensein.sakura.ne.jp/danoni/366_canaria/); あいす's tentative ★×28? / 60-level rating is preserved separately |
| Dancing at Dusk / Retrospective | ★×13 → 13 / Challenge | [Work page](https://vorhandensein.sakura.ne.jp/danoni/295_dancing/), hosting author izkdic's 40-level scale |
| ふしぎなひかり / 9A Easy | ★×2 → 2 / Easy | [Work page](https://vorhandensein.sakura.ne.jp/danoni/291_hikari/), chart table |
| ふしぎなひかり / 9B Normal | ★×6 → 6 / Hard | Same work page, separate table row |

Happy Rainbow and Decipher retain unknown numeric ratings for the captured
festival charts. Happy Rainbow has a related author-catalogue rating for a
separate archive mirror URL; chart equivalence has not been verified here.
MFV2's much later Decipher chart is a different work from the 青空急行 festival
chart and must not donate its rating. Runtime selection counters named
`difficulty`, key counts in legacy `difStep`, initial scroll speed, newly computed tool scores,
note counts and source names such as Hard are not used as source ratings.

Rated charts normally omit `#CHARTNAME`, allowing the menu to show their standard tier and
meter just as it does for Kouhaku. Original labels such as Original, LVUP and
Retrospective remain in `sourceDifficultyLabel` and the unchanged source
reference. Unrated charts retain their source names for identity. Rated charts sharing a song,
key mode and identical meter also retain their names, recorded as
`preservedDuplicateLabel: true`; otherwise their identities would be ambiguous.
The [v2 admission](danoni-corpus-v2-admission.md) documents the new source ratings
and the equal-meter 灼冬赫夜 pair.

The gen3 collaborators publish explicit integer levels on タイヨウノハナ and
G Free's work pages; their stated tool origin is retained as the source scale.
Those published integers reach the same classifier unchanged. Cynthia's decimal
page levels remain separate evidence: no star count or integer meter is inferred
from them. Its レク chart has an exact catalogue integer 6, used independently.
The halted 鉋モドキ selection is not exported, despite its published level 16.

## Review contract and usage

Use `prepare_pack.py --ratings-review REVIEW.json`, or bind the same file as
`policy.ratings: {path, sha256}` in a normalized export-registry recipe. The
registry's evidence inventory additionally covers every copied source body.
No normalized manifest, original dos, runtime reference or musical-grid review
is rewritten. Reviews live under
`~/Music/DanOni/reviews/source-ratings-20261010/<work>/`.

Review schema `ddi-reviewed-source-ratings/1`:

```json
{
  "schema": "ddi-reviewed-source-ratings/1",
  "sourceManifestSha256": "<normalized manifest SHA-256>",
  "charts": {
    "<normalized chart ID>": {
      "chartIdentity": {"mode": "5", "label": "LVUP"},
      "ratings": [{
        "notation": "star-range",
        "token": "★→★★★★★★★★★★ ★★",
        "excerpt": "LVUP ★→★★★★★★★★★★ ★★",
        "encoding": "euc-jp",
        "scale": "Publisher's original star scale",
        "authority": "Original chart maker",
        "evidence": {
          "path": "evidence/<hash>.html",
          "sha256": "<original response-body SHA-256>",
          "url": "https://example.org/work/",
          "retrievedAt": "YYYY-MM-DD or captured UTC timestamp"
        }
      }],
      "selectedRating": 0,
      "selectionReason": "Use the maximum of the published progression"
    }
  }
}
```

Chart IDs form a subset of the normalized manifest, with exact mode and original
label matching. HTML decoding is explicit and strict, including CP932 and EUC-JP
legacy pages. A data-only HTML parser excludes script/style/textarea/title text;
NFKC and whitespace normalization produce the visible text. The selected excerpt
must occur exactly once; the selected rating token must occur once inside it
(integer tokens use digit boundaries). This prevents harvesting a decorative
site star, speed number or another chart's rating from an ambiguous fragment.
The review still supplies the human-verified chart/authority association; it is
not a claim that arbitrary page layouts can be mapped automatically.

Supported notations are `filled-stars` (count ★, ignore ☆), `star-number`
(`★×18`, `☆08`, fullwidth digits), `integer` and `star-range`. Result fields are
`kind`, `minimum`, `maximum`, `rawNotation`, `notation` and `uncertain`.
Question-marked alternatives are retained but cannot set the meter. Each result
also retains scale, authority, excerpt and evidence. Invalid/reversed ranges,
changed source bytes, unknown chart IDs, mismatched labels, ambiguous excerpts
and conflicts with an already extracted runtime rating fail preparation.

The prepared chart's `sourceRating` contains all extracted alternatives,
`selectedRating`, `selectionReason`, `meter` and `meterPolicy`. For a range, the
factual endpoints stay intact and `meterPolicy` explicitly records the derived
maximum choice. The prepared report includes `ratingsReviewSha256` and
`ratingsPolicySourceSha256`; it preserves `ratings/review.json` and its original HTML evidence below
`ratings/`, outside the playable song folders. Third-party pages and music stay local and
are never committed or included in test fixtures.

## Validation

The game's actual SSC parser checks standard tiers, meters and label behavior;
source note/hold timing and compensated movement still undergo the existing
independent checks. The previous generation's fresh-profile Chrome import verified all 15 single-song
ratings, both unrated charts, 19 banners, 23 charts and 13,228 judged notes.
BPMs, beat/color grids, scroll maps, previews, banners and audio are unchanged
from the previous accepted generation. Kouhaku's ZIP is byte-identical. The
previous generation is archived before promotion.
