# Presentation follow-up for the expanded corpus

Investigation: 2026-10-10. The registration-gated original Straight Soul has
been removed from the **active** local registry and source inventory at the
maintainer's request. Its acquisitions and exclusion record are retained;
its independently captured HTML5 port remains eligible. The maintainer may
expand the corpus in another session: updates must reread the latest inventory
and preserve unrelated new entries rather than replacing it with a fixed list.

## Ratings missed by native projection

The family adapters checked initialized runtime labels, not the surrounding
page's rating. This missed genuine published star metadata. Existing cached
pages, requiring no new downloads, give:

| Work | Published rating | Possible presentation |
| --- | --- | --- |
| [FTHosyu](http://eucaly.net/~iso50/onigiri3.html) | Four filled stars, one outline | Source count 4; shared tier heuristic: Medium |
| [party 4u](http://eucaly.net/~iso50/onigiri5.html) | Four filled stars, one outline | Source count 4; Medium |
| [felys](http://eucaly.net/~iso50/10/onigiri10.html) | Eight filled stars | Source count 8; Challenge |
| [RE:RAVE](https://web.archive.org/web/20190314171455/http://www.geocities.jp/naotopage2/game/re_rave/re_rave.htm), Hard | Fourteen filled stars | Source count 14; Challenge |
| RE:RAVE, LVUP | Explicit progression from 1 to 12 | Preserve both endpoints; a peak-rating policy can classify from 12, explicitly as a derived choice |
| [FAKE800](https://mfv2.sakura.ne.jp/do/__Flash/mfv2_005/danoni/DanOni/FAKE800/DO.FAKE800.html) | One filled star and two outlines | Preserve publisher scale/context before deciding whether the common tier thresholds are appropriate |

iso's cached index also describes the easier Ride Out variant as four stars
while rendering four filled stars and one outline. That corroborates counting
filled stars rather than inventing a half-star from an outline. Ratings are
publisher judgments, not a universal cross-site difficulty scale. A decorative
star in a site title, maximum-gauge message or navigation marker is not a rating.

These findings do **not** rewrite immutable native references. Current playable
artifacts still have the earlier null ratings / source labels. The next generic
metadata step should read cached work pages and the exact matching index row,
attach source path/hash/URL and selected chart identity, and preserve raw text,
count or range, scale and extraction method. Conflicting/multiple chart rows
must be resolved explicitly. The existing `1,3,5,7` classifier can consume a
proven source count, with free-text names retained separately for identity.
For a range, keep the range factual and record the derived peak choice outside
extraction. Source-scale differences must remain visible.

Some works supply no numerical rating. In the captured CW works, `Hard`, `Skip`
and `Retrospective` are source names. Their references report difficulty 0,
meaning no supplied level, not a recovered zero-star chart. In `difData`, the
third comma-separated number is **initial speed**; it must never be reused as
stars. An optional actual level belongs in `name::maker::level`.
[Format table and sources](formats-ffr-danoni.md#b1-chart-format).

A chart-derived rating is a separate possible fallback. It should account for
active duration, sustained/peak density, chords, holds, repeated-lane demands
and key mode; report it as estimated and calibrate it against reference charts.
Mapping a label such as Hard straight to an invented star count would lose the
source/estimate distinction.

## Estimate every eligible song; validate application separately

All ten currently normalized native works were analyzed with the existing
generic `analyze_tempo.py`, without song-name hints. Reports remain local at
`~/Music/DanOni/reviews/presentation-study-20261010/<work>-tempo.json`.
Kouhaku's accepted musical reviews were not replaced.

| Work | Leading chart candidate, rounded | Audio comparison / interpretation |
| --- | --- | --- |
| Higan | 154.990 | Audio hypotheses near 154–155; stable portions available |
| RE:RAVE | 145.805 | Audio near 145.58; stable portions available |
| Happy Rainbow | 200.227 | Audio near 198.77 with slower aliases; stable portions available |
| FAKE800 | 200.337 | Audio near 198.77 / 100.35; stable portions available |
| Destined Marionette | 186.001 | Audio near 186.23; current window-consensus threshold does not accept a dominant grid |
| felys | 179.338 | Audio near 178.21; weaker chart consensus |
| FTHosyu | 139.348 | Audio near 139.67; poor global chart lattice |
| party 4u | 158.262 | Audio hypothesis near 161.50; poor global chart lattice |
| Decipher | 109.593 | Audio near 145.58–147.66; the leading chart subdivision alias is not an established musical pulse |

These are fitted hypotheses, not assertions that the rounded number is the
correct timing BPM. The current analyzer's local window search is seeded by a
leading chart candidate; competing audio-supported pulse aliases should also
seed that search. Kouhaku itself illustrates why this matters: the strongest
unhinted lattice candidate for 桃源郷 can be about 106.5 while audio supports about
159. The shared algorithm should rank joint chart/audio evidence rather than
blindly select the largest coherence score.

Run analysis by default for every supported normalized song. Apply a consistent
musical grid automatically when audio and chart evidence support its tempo,
phase and dominant portions; otherwise retain alternatives and a reason for
review. Estimation and application are separate: uncertain results stay reviewable
without blocking analysis of the rest of the corpus. Saved CW packages first need an audio-relative normalization
bridge to use the same analyzer/exporter; do not treat their raw padded engine
frames as seconds.

For application to SSC, use the existing below-10-ms endpoint error budget,
coarse rhythmic snapping where supported, and fine-tick fallback elsewhere.
Reject collisions, invalid hold relationships and inconsistent shared-music
charts. Fit actual tempo segments when a constant tempo cannot explain the
music; do not keep loosening acceptance until a number passes. Tempo evidence,
quantization movement and source audio sync are different measurements.
Measured `#SCROLLS` compensation must continue to preserve native motion and
multiply the user's speed setting, independent of the inferred musical BPM.

Missing menu BPMs are explained by packaging: normalized native works currently
use exact synthetic 75-BPM SSCs, whereas the CW packages retain native DanOni
format and unspecified musical BPM. The shared library intentionally leaves a
native work's BPM blank when none is supplied instead of advertising the
synthetic clock as musical tempo. Supplying an estimated display BPM alone can
fix that label; note colouring additionally needs the musical grid/phase path.
[Existing timing/presentation contract](danoni-postprocess.md).

## Covers with source meaning

The [artwork study](danoni-artwork.md) already distinguishes source title graphics,
link banners, navigation badges and generated imagery. Its typographic template
is a fallback; audio-hash-derived colours do not express the song's subject.
A meaningful generic pipeline should use these priorities:

1. A genuine associated song/title image, when the cached page/runtime establishes
   its role. Preserve original bytes and record any derivative crop.
2. A source-informed title card: use the work's title screen/wordmark, palette
   and visual motifs, with crisp title/artist typography composed separately.
   Avoid shrinking a whole settings/gameplay screenshot into a banner. Native
   Ruffle can supply runtime frames; larger clean captures need a reviewed
   viewport/rendering change, not enlargement of the existing 320×240 pixels.
3. A consistent typographic fallback with verified title, artist and chart-author
   credits. Do not invent a genre, subject or composer from a song name alone.

Optional generated illustration should follow a reviewed brief drawn from the
source visuals/title and musical character. Treat it as new art, keep Japanese
text in the deterministic layout layer, and retain generation provenance. It
should not be silently described as original cover art.

Use the same cover across a song's difficulties. Emit a wide `banner.png` and
`#BANNER` for the game; a square `cover.png` / `#JACKET` can be an additional
interchange asset. Pack art belongs directly inside the collection folder.
Kouhaku can follow its source event's red/white identity; the mixed single-song
collection can use a coordinated cover collage or a simple collection title,
without suggesting all its songs share an author. Preserve readable safe areas
at the actual 160×50 song and 418×164 pack menu crops. A local contact sheet is
an appropriate review artifact before adding artwork to generated ZIPs.

Implementation order: cached-page numeric metadata, automatic analysis and
joint-evidence musical-grid application (including the CW bridge), then a
source-informed artwork contact sheet and reviewed pack integration. The exact
source references, accepted Kouhaku bytes and native-motion policy remain the
comparison baseline throughout.


The maintainer subsequently preferred native source-title panels and plain
white-on-black fallback cards. That direction is now implemented and packaged
for the 14 normalized native songs; see [the artwork handover](danoni-artwork.md#original-selector-panels-and-plain-fallback-2026-10-10).
It supersedes the optional decorative/illustration direction as the default.
Page ratings and musical-grid application remain separate pending work.
