# DanOni artwork and title study

Local investigation and prototype, 2026-10-09. The conversion policies remain
separate from visual presentation. No existing playable ZIP, source metadata,
note timing, music, preview or scroll setting was changed for this study.

## What was generic in the accepted Kouhaku pack

The pickup-preview algorithm used its shared defaults, without song names,
per-song starts or pack-specific thresholds. Star classification used the shared
`1,3,5,7` upper bounds; Vorbis used quality 5; the measured-scroll correction
factor was 1.0. These are presentation policies, not facts recovered from music.

The entire extraction is **not** automatic for arbitrary Flash works:
[`kouhaku.py`](danoni-consolidation.md#retired-tools) (retired) pins a reviewed SWF hash,
menu routes, selection metadata, initialized arrays and audio origin. Musical
BPM/phase inputs were reviewed per song, including the emy and 北風 tempo aliases;
scroll values came from source-motion measurements. The shared preparer has no
Kouhaku rules, but it consumes those source-specific observations and reviews.

## Recoverable source visuals

The [original page](http://eucaly.net/~iso50/kouhaku.html) names the third 紅白FLASH合戦
and Dancing☆Onigiri. Its source SWF is the maintainer's already acquired
`2004kouhaku.swf`, SHA-256
`e14e472911b6603ddcc58bd40049d0b05cfa986a2240209dd848a516cf40fed2`.
No network acquisition was necessary.

Restricted native Ruffle rendered the original song-selection states for all
five songs. They supply wordmarks, including **桃源郷 -Remix-**, while the runtime
chart title is **桃源郷**. These are different source representations, not a reason
to silently rewrite the canonical song title. The page and runtime also spell
ころっけ differently; retain both with their origin. The initial 桃源郷 capture
has blank artist/difficulty fields, so a screenshot is not a metadata oracle.
The extraction's reviewed metadata remains authoritative for these cards.

Static inventory finds two `DefineBitsJPEG2` images (character IDs 42 and 45),
three `DefineBitsLossless` and two `DefineBitsLossless2` tags, alongside vector
shapes, embedded fonts, static text and editable text. The two JPEGs are small
website/link banners, not five album covers. Button actions link to musician
sites. Do not choose a cover merely because an image exists in the movie.
Lossless bitmap decoding and isolating vector wordmarks are still future work.

Both JPEG payloads begin with the erroneous SWF marker pair `FF D9 FF D8`, then
the actual JPEG. A normal image decoder rejected them. Removing exactly those
first four bytes made both decode, retaining the remaining compressed bytes.
The untouched payloads, normalized derivatives and hashes are saved locally.
This behavior is explained by Ruffle's
[`remove_invalid_jpeg_data`](https://github.com/ruffle-rs/ruffle/blob/89a7049965f3090e706265670df309a5128233ac/render/src/utils.rs).
Other movies can contain the marker pair inside the JPEG header; blindly
stripping four bytes from every image would be incorrect.

The current native worker renders 320×240. These menu captures are useful
reference material; they do not recover the source's original stage resolution,
original Flash rasterization, or a clean, scalable wordmark. A faithful visual
export should retain the capture configuration, viewport, native snapshot,
worker identity and any explicitly reviewed crop. Larger rendering needs a
separate worker change and validation; enlarging a PNG cannot restore detail.

For HTML5 works, source images/fonts/scripts are already catalogued by the
browser dumper. Empty image `chartIds` means unresolved usage. A background or
title animation, lyric image, navigation badge, musician-site banner and song
cover are distinct roles. Keep the original asset bytes; choose a representative
runtime frame only when a static image does not express the title treatment.

## Generic typographic fallback prototype

[`artwork.mjs`](danoni-consolidation.md#retired-tools) (retired) consumes an existing
`ddi-playable-pack/experimental-1` manifest and its `references/<song-id>-0.json`
files. It creates abstract lane motifs and typography from the song title and
artist, without decoding music, executing source scripts, requesting network
resources, or changing a simfile. It uses the existing pinned `playwright-core`
and installed Chrome; no dependency or font download was added. Code is MIT
under the catch-all in `REUSE.toml`.

Palettes/motifs derive from source-audio identities; the pack identity derives
from the caller's title and sorted song identities. They do not change when
only preview or timing policies change. They carry no inferred genre, emotion,
illustration subject or difficulty meaning. Normal/Hard share song artwork.
Japanese text is measured before rendering, wraps at word/grapheme boundaries,
and preserves the full supplied title. Oversized titles/credits require review
instead of silent truncation. XML/HTML text is escaped.

From the research worktree, with tool dependencies already installed:

```sh
node tools/danoni-dump/artwork.mjs \
  "$HOME/Music/DanOni/legacy-pilot/exports/2004-kouhaku-pickup-previews-v2/manifest.json" \
  "$HOME/Music/DanOni/legacy-pilot/artwork/new-study" \
  '紅白FLASH合戦 2004'
```

The parent must exist and the output directory must be new and outside every
Git checkout. The third argument is an explicit display label, not a recovered
official pack title. The example is a shortened label based on the source page;
retain its original heading separately before publishing any catalogue.

```text
artwork.json                       # provenance and output inventory
index.html                         # local gallery, no external resources
pack/pack.svg, pack/pack.png        # 836×328, current pack-header ratio
pack/cover.svg, pack/cover.png      # 768×768
songs/<id>/banner.svg, banner.png   # 640×200, current song-banner ratio
songs/<id>/cover.svg, cover.png     # 768×768
```

`artwork.json` uses schema `ddi-artwork-study/experimental-1`:

| Field | Meaning |
| --- | --- |
| `generatedAt` | UTC ISO-8601 artifact creation time, not source retrieval time |
| `sourceManifest`, `sourceManifestSha256` | Local prepared-pack manifest identity |
| `generatorSha256`, `template` | Tool bytes and template identity (`typographic-lanes/1`) |
| `browser`, `platform`, `fontStack`, `fontReproducibility` | Rendering environment; installed fonts are not distributed or pinned |
| `packTitle` | `{value, origin:"caller-provided display label"}` |
| `originalAssetsUsed`, `networkRequests` | `false`, `0` for this generated study |
| `records[]` | One record per output role; IDs remain source song IDs (`pack` for pack art), roles distinguish them |
| `records[].title`, `artist` | Untranslated song metadata; pack artist line is the game-format label |
| `records[].svg`, `png` | Paths relative to the artwork directory |
| `records[].svgSha256`, `pngSha256` | Hashes of generated files |
| `records[].dimensions` | `[width,height]` in output pixels |
| `records[].typography` | `{lines:string[],size:number}`; title font size in SVG pixels |
| `records[].sourceReferenceSha256` | Prepared reference-file identity, or null for the pack |

Exact PNG pixels can differ with installed fonts/Chrome versions. The SVG keeps
editable text but still needs compatible Japanese fonts to render. Pinning an
explicitly redistributable Japanese font and its license is a later production
step. Generated art does not change the rights to the source music or graphics.
All real-work study artifacts remain local.

The inspected comparison gallery lives at
`~/Music/DanOni/legacy-pilot/artwork/2004-kouhaku-study-v1/index.html`.
It also contains `original/` with the five full menu captures, input configs,
source SWF, raw snapshots, execution reports, two untouched JPEG payloads and
normalized viewing derivatives. `original/provenance.json` records the source
URL/hash, unknown original SWF acquisition date, capture dates and conversions.
These third-party files are not repository fixtures.

## Export integration contract and next experiments

The current `main` importer finds pack names from parent folders (or the ZIP
name as fallback) and pack images directly inside the pack folder, preferring
PNG. Its menu crops pack images to 418:164 and song banners to 160:50. This is
based on read-only inspection of `crates/library/src/pack.rs`, `crates/app/src/import/mod.rs`,
`crates/app/src/components/song_select.rs` and `styles.css` on 2026-10-09; the research
branch's older app does not yet have that full pack-art UI.

A future exporter can use this existing contract:

```text
<reviewed-pack-name>/
  pack.png                         # one image here: pack selection artwork
  <song-id>/
    song.ssc                       # #BANNER:banner.png;
    music.ogg
    banner.png
    cover.png                      # optional #JACKET, for other software
```

Use the wide banner for this game's menu. `#JACKET` is parsed but does not
currently supply its song-menu image. Do not add `#BACKGROUND` automatically:
a menu cover is not necessarily a suitable gameplay background. Keep square
covers out of the pack folder so they cannot accidentally win pack-image
selection. Adding a containing pack folder changes imported pack/song identity;
plan an explicit reimport or use a ZIP display name without relocating song
paths. This study does not package or mutate any accepted ZIP.

The next useful experiments are:

1. **Source-first asset review:** associate title graphics with the selected
   song, exclude link/navigation images, and capture/crop an approved title
   state with provenance. Store runtime title, page title and graphic wording
   separately, along with raw `musicTitle`/`tuning` and source URLs for HTML5.
2. **Template review at actual menu size:** compare these wide banners with
   source wordmarks, tune legibility and palette, then support a reviewed
   artwork manifest in the pack preparer. This can stay completely generic.
3. **Optional illustration layer:** a commissioned or generated illustration
   can sit beneath the same deterministic text layer. Keep titles/Japanese
   typography separate so spelling and credits remain exact. Any generated
   imagery should be recorded as new artwork rather than recovered source art.

A later reviewed artwork manifest should bind to the source export hash and
song IDs, identify each role/path/hash, distinguish original, cropped,
runtime-rendered and generated assets, and record source URL/date, creator,
license status, capture frame/viewport or generation recipe and exact
modifications. For multi-chart songs, share the song cover instead of using a
difficulty-specific cover unless the source actually supplies one. The importer
session need not implement a new title/art format to consume the standard
folder layout and `#BANNER` references above.

## Original selector panels and plain fallback (2026-10-10)

The maintainer prefers the rectangular song-title treatments already present in
Kouhaku's selector, including their fonts, spacing and placement. All five are
now recovered through **native Ruffle**, without browser execution or new
acquisition. A named `music_select` MovieClip contains the complete title and
border. Its selected timeline frames are 1, 31, 61, 91 and 121 in source song
order; these match the earlier menu captures. The source's graphic wording,
including `-Remix-` and its ころっけ spelling, remains intact without changing
canonical song metadata.

The worker adds bounded `renderScale` (integer 1–4, default 1), scaling the
320×240 offscreen target and its device scale together. Recorded clicks retain
logical 320×240 coordinates: move, press and release are mapped to output pixels.
It redraws source vectors/fonts at 1280×960 rather than enlarging an old image.
Snapshots expose `stageToViewport: [a,b,c,d,tx,ty]`, a native affine transform
from stage pixels to captured pixels; existing stage bounds retain their units.
The original synthetic integration checks high-resolution pixels and both mouse
press/release coordinates, together with the unchanged sandbox checks.

[`capture_title.py`](danoni-consolidation.md#retired-tools) (retired) consumes a
successful restricted `snapshot.json` / `run.json` and its rendered PPM. It
requires a unique visible named MovieClip (including visible ancestors), complete
state/audit, and optionally the reviewed selected timeline frame. It transforms
all four bounds corners, rounds outward and adds one pixel for edge antialiasing.
An out-of-viewport or ambiguous crop fails. FFmpeg emits a lossless `panel.png`
and an aspect-preserving 640×200 `banner.png` on a black matte. It does not retype
the original wordmark. The current source crop is 766×174 output pixels.

```sh
python3 tools/danoni-dump/ruffle/run.py WORKER CONFIG_JSON NEW_CAPTURE
python3 tools/danoni-dump/ruffle/capture_title.py \
  NEW_CAPTURE/snapshot.json 90 music_select NEW_PANEL_DIRECTORY \
  --expected-clip-frame 1
```

`panel.json` uses `ddi-native-title-panel/1`: source SWF/page URLs and hash,
capture date, native worker/build identity, snapshot/run/image hashes,
`captureFrame`, `clipName`, `clipMovieFrame`, native stage bounds/viewport matrix,
`cropPixels: [x,y,width,height]`, banner dimensions, background, `titleReplaced:
false`, exact FFmpeg commands/version and PNG hashes. Original captures remain
local at `~/Music/DanOni/reviews/kouhaku/title-panels-v2/`; `contact-sheet.png`
shows the five recovered panels. Earlier failed experiments remain separate.

For sources without an associated usable panel, `artwork.mjs` now defaults to
`plain-title/1`: centered original song title in white on black, with verified
artist credits only when available. The title is not translated or shortened.
There are no decorative lane motifs in this default. `--background '#rrggbb'`
and `--foreground '#rrggbb'` allow a known source-page scheme; `--style lanes`
retains the earlier study template. PNG rasterization still uses installed Chrome
and fonts in a separate, network-denied composition step. SWF panel extraction
itself uses no Playwright. SVG/PNG/environment provenance remains recorded.

## Reviewed banner integration

The preparer accepts `--artwork-review FILE`; registry recipes use
`policy.artwork: {path,sha256}`. Review schema `ddi-reviewed-song-artwork/1`:

```json
{
  "schema": "ddi-reviewed-song-artwork/1",
  "sourceManifestSha256": "<exact normalized manifest SHA-256>",
  "charts": {
    "tougenkyo": {
      "origin": "native-title-panel",
      "banner": {"path": "songs/tougenkyo/banner.png", "sha256": "<PNG SHA-256>"},
      "evidence": {"path": "songs/tougenkyo/panel.json", "sha256": "<JSON SHA-256>"}
    }
  }
}
```

Paths are contained, regular files relative to the review, with no symlinks.
Each covered chart needs a 640×200 PNG and matching evidence. Native evidence
must bind the source SWF, resulting PNG and unchanged title pixels. `plain-title`
evidence must be the matching `ddi-artwork-study/experimental-1` record with
`template: plain-title/1`. Unknown chart IDs, changed bytes and conflicting or
missing banners among difficulties sharing music are rejected. The actual
Kouhaku review covers all six charts; both 桃源郷 difficulties share one panel.

Output song folders add `banner.png` and `#BANNER:banner.png;`. Prepared manifests
record banner path/hash, origin, dimensions, evidence hash and review hash.
The game-parser verifier resolves the tag and checks the image dimensions.
Accepted Kouhaku simfile bytes must remain identical after removing **only**
that exact new banner header; music remains byte-identical. Note/hold timing,
scroll, BPM, previews and ratings retain the existing independent checks.
Original source/reference files are not rewritten and no gameplay background
is assigned from a cover.

The current local generation supplies five original Kouhaku panels and nine
plain native-work banners: **14 songs with artwork** in that generation. The
subsequent [CW bridge and default fitting](danoni-default-tempo.md) adds the five
remaining title banners via SSC, giving all 19 songs artwork. Native game CW
imports still lack a banner resolver; converting saved verified references avoids
requiring a game change. No game files are changed here.

Only **9A and 9B** are eligible nine-key exports. The current Hikari package
contains those two selected charts; 9i remains solely an unsupported row in the
retained original effective dos, explaining the import warning. The new synthetic
packaging regression proves 9i and 9t charts/media references cannot be selected
alongside 9A/9B. It does not rewrite original oracle data to remove that warning.
