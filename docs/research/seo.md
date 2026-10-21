# Deployed page SEO audit

Reviewed 2026-10-21 at <https://sugoijan.dev/ditoditoinfinito/>. The live
observations below precede the screenshot metadata changes in this checkout;
they are not a claim that those changes have been deployed or indexed.

## Live observations

| Check | Result |
| --- | --- |
| Anonymous HTML fetch | HTTP 200, `text/html; charset=utf-8`; no `X-Robots-Tag` restriction. |
| Metadata without JavaScript | Title, description, canonical, robots, Open Graph text, Twitter summary card and JSON-LD all present in the initial HTML. |
| Canonical | HTTPS, lowercase path, trailing slash; the slashless URL redirects with 301 to the canonical. |
| Crawl permissions | The **origin-root** `/robots.txt` allows all paths. It points to `/sitemap.xml`, whose sitemap index includes the game's sitemap. |
| Game sitemap | HTTP 200, XML, one canonical page. No image entry before this change. |
| Images | No `og:image`, `twitter:image`, JSON-LD image or screenshot before this change. |
| Crawler user agents | Requests labeled Googlebot and Twitterbot both returned 200. These are access probes, not proof of access from those services' networks. |
| Google Rich Results Test | Live smartphone crawl succeeded, read the metadata, and rendered the actual start menu, including the first-run calibration notice. “No items detected” for rich results. |
| Schema.org validator | Its URL fetch failed. Pasting the JSON-LD extracted from the live response succeeded: one WebPage with a VideoGame main entity, **0 errors and 0 warnings**. |
| OpenGraph.xyz | The public URL scanner reported access blocked. No preview score or successful social-platform fetch was obtained. |

The [Google result](https://search.google.com/test/rich-results/result?id=l8ma4oEaLthBMPgKmTYHNA)
is a point-in-time diagnostic and may expire. Re-run the
[live test](https://search.google.com/test/rich-results?url=https%3A%2F%2Fsugoijan.dev%2Fditoditoinfinito%2F),
[Schema.org validator](https://validator.schema.org/) and
[OpenGraph.xyz scan](https://www.opengraph.xyz/url/https%3A%2F%2Fsugoijan.dev%2Fditoditoinfinito%2F)
after deployment. A scanner failure alone does not establish that real social
crawlers are blocked; investigate the domain's CDN security events if actual
shared links fail. Google succeeded during the same audit.

## Changes prepared

- Publish the existing 1280 × 720 gameplay still, unmodified, at
  `images/readme.png`. Its provenance remains in
  [the screenshot documentation](../screenshots/README.md).
- Supply absolute Open Graph and Twitter image URLs, descriptive alt text,
  PNG type and actual dimensions; use `summary_large_image`.
- Supply the game entity's `image` and `screenshot`, plus the WebPage's
  `image` and `primaryImageOfPage` ImageObject.
- List the screenshot in the existing sitemap's image extension. This helps
  discovery for a game whose visible UI is drawn on a canvas.
- Describe play, keyboard/dance-pad input and bundled/imported songs in plain
  language. Preserve the “dance rhythm game” wording; keep the technical genre
  in the structured description and genres.

All tags remain generated at build time from `site/seo/metadata.toml`.
The generated files are not committed. Tests check the real image dimensions,
its Trunk copy directive, image metadata, optional-image behavior and XML
escaping. No gameplay or rendering code changes are needed.

Local validation: all 17 `cargo test -p xtask` tests passed; `regen-seo`,
Rust formatting and `git diff --check` passed. A focused Trunk build using
the page's five SEO inline/copy directives under `/ditoditoinfinito/` produced
matching HTML/JSON-LD/XML and a byte-identical screenshot at the declared
path and dimensions. This was a metadata packaging check, not a full wasm
rebuild. `reuse lint` could not start because the installed launcher points
to a missing pipx Python interpreter; no new image or license was introduced.
The updated generated JSON-LD also passed Schema.org's code-input validation
with **0 errors and 0 warnings**, including the screenshot and ImageObject.
That validates its vocabulary and structure, not the availability of the
image URL before deployment.

## Interpretation and follow-up

**Rich-result eligibility and indexing are different.** Google does not offer
a software-app rich result for `VideoGame` alone. Its software-app feature
also requires a real review or aggregate rating and an offer price. Do not
invent a review or change types merely to obtain a green rich-result test.
Keep truthful game markup; consider the additional application type only if
there is real, visible review content to support that feature.
Source: [Google software-app structured data](https://developers.google.com/search/docs/appearance/structured-data/software-app).

**Image metadata is a preference, not a guarantee.** Google documents
`primaryImageOfPage`, main-entity `image`, and `og:image` as signals for image
selection. It still chooses the displayed image and snippet automatically.
The page already permits large image previews. The screenshot's real gameplay
is more representative than the favicon; a bespoke social crop is optional.
Sources: [Google image guidance](https://developers.google.com/search/docs/appearance/google-images),
[image sitemaps](https://developers.google.com/search/docs/crawling-indexing/sitemaps/image-sitemaps),
[Open Graph image properties](https://ogp.me/#structured).

**Do not add keyword stuffing or fake routes.** Google ignores `meta keywords`.
The game's hash routes are application states, not independently served pages;
the sitemap should continue listing only the canonical document. A project-level
`robots.txt` under `/ditoditoinfinito/` would not control crawling. The existing
origin-root policy and sitemap discovery already work.
Sources: [Google supported metadata](https://developers.google.com/search/docs/crawling-indexing/special-tags),
[robots.txt scope](https://developers.google.com/search/docs/crawling-indexing/robots/intro).

**After deployment:** check the image URL returns HTTP 200 with `image/png`,
re-run the live tests, and use the verified property's Search Console URL
Inspection to check Google's selected canonical, indexing status and last
crawl; request indexing there if appropriate. This audit did not access the
owner's Search Console, submit an indexing request, or verify index inclusion.
An anonymous live test cannot establish those facts.

**Optional larger improvement:** a visible HTML About/help section with a
gameplay image, controls and supported import formats would give readers and
crawlers more descriptive content. Today the static body has a concise
screen-reader summary; the rest is a canvas. Keep any future content useful
and accessible to players rather than adding hidden SEO paragraphs.

## Search terminology follow-up (2026-10-21)

The maintainer wants accurate references to DDR, StepMania and Pump It Up
where they help discovery. This supersedes the blanket ban on naming other
games in public copy; `AGENTS.md` now permits factual compatibility and
gameplay references. There are no user ratings. A possible Steam release
is undecided and is not advertised in the metadata.

Google Trends was inspected directly, using **Worldwide, Past 12 months,
Web Search**, with search terms rather than topics. The values below are
the UI's rounded **average interest indexes**, not monthly search counts,
market shares, ranking difficulty, or traffic forecasts. Each comparison
has its own 0–100 scale; do not compare numbers across the two tables.
The latest weekly bucket was still partial. Multiword terms here are not
quoted exact phrases; the result includes matching longer queries.

[Genre comparison, all categories](https://trends.google.com/trends/explore?date=today%2012-m&q=rhythm%20game,dance%20rhythm%20game,StepMania,4K%20rhythm%20game,VSRG&hl=en):

| Search term | Average index |
| --- | ---: |
| rhythm game | 60 |
| StepMania | 7 |
| dance rhythm game | 1 |
| VSRG | 1 |
| 4K rhythm game | 0 |

[Brand and genre comparison, Games category](https://trends.google.com/trends/explore?cat=8&date=today%2012-m&q=StepMania,Dance%20Dance%20Revolution,Pump%20It%20Up,4k%20rhythm%20games,rhythm%20games&hl=en):

| Search term | Average index |
| --- | ---: |
| Pump It Up | 71 |
| Dance Dance Revolution | 51 |
| rhythm games | 32 |
| StepMania | 4 |
| 4k rhythm games | 0 |

The Games filter reduces ambiguity but does not remove all of it: Pump It
Up's related queries still included a phrase-meaning query as well as
`pump it up phoenix 2`. Category classification also changes the sample for
StepMania. Do not infer that Pump gameplay has exactly the displayed ratio
of demand relative to StepMania. Bare `DDR` was deliberately not compared
because it has unrelated meanings. Full names and qualified phrases are
more interpretable for this purpose.

A zero can reflect low volume and rounding, not absence of interest.
Trends uses samples and normalization; the relative result can change
with geography, time, category and term/topic choice. Sources:
[Trends data FAQ](https://support.google.com/trends/answer/4365533),
[comparison semantics](https://support.google.com/trends/answer/4359550).

### Copy decision

- **Lead with dance rhythm game.** It contains the useful broad phrase
  `rhythm game` while identifying this game's particular kind of play.
  Low interest in the longer term is not a reason to drop the accurate
  adjective. Broader interest alone does not establish achievable rankings.
- **Put StepMania simfiles in the short description.** This is a concrete
  capability, backed by the `.sm`/`.ssc` importers, with a more specific
  audience than a general search for another game's name.
- **Keep 4K and VSRG as secondary vocabulary.** Explain four-key and vertical
  scrolling; do not describe the entire game as limited to four keys.
- **Use Pump It Up and DDR in the longer description with context.** The
  game plays five-panel Pump charts and has a DDR A ruleset. That supports
  `Pump It Up-style five-panel charts` and `Dance Dance Revolution (DDR) A
  ruleset`; it does not support claiming to be either official game or to
  include their commercial catalogs. Pump holds follow StepMania semantics;
  the DDR timing windows are community approximations. See
  `docs/plans/pump.md` and `crates/engine/src/rules/presets.rs`.
- **Do not add brand names as genres or a meta-keywords list.** The useful
  work is accurate explanatory copy. Google ignores `meta keywords`;
  structured-data genres are descriptions, not a promised ranking boost.

The short meta/Open Graph/Twitter description now emphasizes genre,
controls, StepMania import and layouts. The structured description and the
existing concise accessibility summary explain the DDR and Pump references.
No extra hidden keyword paragraphs or new landing-page UI were added.
After indexing, Search Console's query impressions, clicks and CTR will be
the evidence for this site's actual discovery; Trends cannot predict the
impact of these copy changes. A later visible compatibility/help page
could answer specific questions about importing simfiles and using layouts.
