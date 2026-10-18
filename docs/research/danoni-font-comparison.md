# Sarasa Term J title-font comparison

Local comparison requested on 2026-10-16, after rebasing `danoni-research`
onto main `2ffd27e`. This evaluates the title-art renderer only. The installed
runtime still uses the existing Iosevka Term / Noto Sans CJK JP policy; no game
font assets, existing exports or font installation are changed.

## Candidate and provenance

[Sarasa Gothic](https://github.com/be5invis/Sarasa-Gothic) has several different
Latin styles. **Term J** uses Iosevka Latin and Japanese regional forms; the
Gothic/UI variants use Inter instead. See the upstream
[variant descriptions](https://github.com/be5invis/Sarasa-Gothic/blob/v1.0.42/README.md#what-are-the-names).
The composite includes Source Han Sans CJK. Its full
[release licence](https://github.com/be5invis/Sarasa-Gothic/blob/v1.0.42/LICENSE)
is OFL-1.1, retaining the upstream copyright notices.

The comparison uses the unchanged, hinted Regular face from
[release 1.0.42](https://github.com/be5invis/Sarasa-Gothic/releases/tag/v1.0.42):

| Item | Bytes | SHA-256 |
| --- | ---: | --- |
| `SarasaTermJ-TTF-1.0.42.7z` | 68,289,162 | `c0f3715a31ba77e611f0a9bed7ef5ec61a2519060d89dde65c232086c2a084ac` |
| `SarasaTermJ-Regular.ttf` | 26,342,708 | `7dc20d5793b03d2cafd5846392b1b8dfd6f6bc204db1362f0f608641e9b5a1e4` |

The archive hash matches the official GitHub release API's asset digest.
Extraction uses `7z e SarasaTermJ-TTF-1.0.42.7z -ofont SarasaTermJ-Regular.ttf`.
There is no subsetting, renaming or conversion. Release metadata, licence,
retrieval time and source hashes stay alongside the local font. The candidate
font is not committed. The current two-face baseline totals 16,494,676 bytes;
this complete hinted Sarasa face is about 9.4 MiB larger. This is a file-size
comparison, not a measured renderer memory or performance result.

## Reproduction

The checked-in Rust example consumes an external font file and creates a fresh
external comparison directory. It calls the production `TitleRenderer::card`
for both the baseline and candidate, checks missing glyphs and bounds, and uses
only explicitly supplied font bytes. It loads no system fonts and executes no
work scripts. No additional dependencies are needed.

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --example compare_title_fonts -- \
  /external/font/SarasaTermJ-Regular.ttf /external/new-comparison
```

The output contains:

- `index.html`: all 12 labelled comparisons, with PNG text already rendered.
- `overview.png`: the first four banner pairs at native resolution.
- `banner-00.png` through `banner-11.png`: individual 640×200 banner pairs.
- `small-text.png`: Japanese/Latin, kana and punctuation at 16, 20 and 28 px.
- `comparison.json`: font hash/size, production layout decisions and actual
  glyph-family counts for each banner.

Left is the current packaged Iosevka/Noto pair; right is the candidate alone.
Samples include Crow, Maid, Chinese Tea, The Crow's Newspaper, No Name,
ふしぎなひかり, 桃源郷, 北風のメロディー, ころっけわっしょい,
Japanese regional-form characters, composed/decomposed kana and punctuation.
Credits in the specimen exercise typography; they do not add or revise any
song's provenance or catalogue metadata.

The smaller specimen uses the same usvg/resvg shaping and rasterization as
production banners. It does **not** establish browser/cosmic-text UI rendering
parity or validate every CJK character. Those remain separate application font
checks if Sarasa is selected for the main theme.

## Observed comparison

Local results, font/licence and provenance are retained under
`~/Music/DanOni/reviews/sarasa-term-j-20261016/`; open
`rendered/index.html` for the complete comparison. All 12 banner pairs and the
16/20/28 px specimens render without missing glyphs or clipped text. Visual
inspection of the overview, small-text and regional/combining-character sheets
shows readable Japanese and Latin in both candidates. These are local title
samples, not a complete coverage or cross-platform raster certification.

The useful distinction is the Latin in mixed runs. The current renderer can
choose Noto for an entire Japanese/Latin run, while another line uses Iosevka.
Sarasa uses one composite face throughout, keeping the narrow Latin appearance
in Chinese Tea and The Crow's Newspaper and in their credits. Standalone Crow
looks very close. The Japanese letterforms are broadly similar, as expected
from their related Source Han/Noto lineage; the spacing is not identical.

The regional-character spacing specimen takes two lines with Sarasa and one
with the current pair, at the same selected 52 px size. The existing production
wrapper also retains a leading space on Sarasa's second Newspaper line. This
comparison preserves that behavior instead of changing layout while judging the
font. A later adoption should fix that whitespace edge case and review wrapping
on the complete title set. No smaller-size, different-weight or subset decision
is implied by the Regular comparison.

## Rebase checkpoint

All 73 research commits replayed cleanly onto main `2ffd27e`. The pre-rebase tip
`6acfe49` remains at local `backup/danoni-before-main-20261016`. The research tool
sources were unchanged by the replay. The standalone SDK lockfile needed two
local dependency edges from main (`ddi-library` → `ddi-engine`, and
`ddi-platform` → `serde_json`); no third-party dependency version changed.
That SDK lockfile identity change requires a fresh SDK/runtime installation;
it does not alter the Ruffle worker's source identity. Historical captures and
installed-prefix receipts remain historical evidence.

The font candidate and generated images remain local. The only new reusable
code is the comparison example; the production font policy is unchanged pending
visual selection. The branch history was rewritten, so coordinate a
`--force-with-lease` push and the other machine's checkout before resuming work.

## Validation

After the rebase, `just check` passes (native checks, wasm check, SEO regeneration
and REUSE lint). The standalone SDK's default-feature test suite passes 280 tests;
the `--no-default-features` suite passes 96 tests. Strict
`cargo clippy --all-targets -- -D warnings` passes with both feature
configurations. Optional external
corpus/runtime gates were not enabled: these results do not replace Linux runtime
or corpus validation, nor certify a newly installed runtime prefix.
