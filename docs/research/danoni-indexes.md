# DanOni index discovery and single-song corpus

Research date: 2026-10-09. Prioritize **one song per SWF**, with one or several
difficulties, alongside HTML5 works published the same way. The Kouhaku
multi-song SWF remains a collection case rather than the default adapter model.
This is a bounded Japanese-language survey and a local source corpus, not an
estimate of ecosystem-wide frequencies or a claim of universal SWF conversion.

Gameplay targets are **5, 7, 7i, 9A and 9B**, with 5-key first. Exclude 9t,
other nine-lane modes, larger modes and work-defined/redefined layouts from
playable exports. Lane count alone does not establish a supported mode.
Preserve unsupported definitions as source evidence, and report them per chart;
a mixed-mode song can still have supported charts. Historical `DP`/`9` aliases
need the reviewed family mapping, never a blanket “nine keys” conversion.
This follows PLAN decision 27 and `PLAYED_DANONI_MODES`; neither was changed.

## Reference publication routes

| Index input | Individual work examples | Verified route and value |
| --- | --- | --- |
| [気まぐれな場所 / iso's Flash list](http://eucaly.net/~iso50/flash.html) | [FTHosyu](http://eucaly.net/~iso50/onigiri3.html), [party 4u](http://eucaly.net/~iso50/onigiri5.html), [felys](http://eucaly.net/~iso50/10/onigiri10.html) | Publisher dates 2004-03-29, 2004-04-23 and 2004-07-08. Normal work-list anchor navigation reaches each single-song page and its embedded SWF. All three originals and title captures acquired locally. |
| [NAOTOPage catalogue](https://cw7.sakura.ne.jp/lst/ds0174/) | [RE:RAVE, archived page](https://web.archive.org/web/20190314171455/http://www.geocities.jp/naotopage2/game/re_rave/re_rave.htm) | Catalogue lists 5-key LVUP/Hard and a 2005-07-28 update, associated with the 2005 summer event. Archived page, SWF and external `dos.txt` acquired and rendered offline. Also lists The Five (2004-12-19), Holy Moon (2005-05-09) and Colored (2006-03-19); these additional destinations are discovery candidates, not acquired references. |
| [7key Festival 2006 work list](https://cw7.sakura.ne.jp/event/7fes06/works.html) | [HAPPY RAINBOW](https://cw7.sakura.ne.jp/event/7fes06/works/31hpr/main.html), [Decipher](https://cw7.sakura.ne.jp/event/7fes06/works/15decipher/main.html) | Event list → individually hosted pages → `main.swf` plus `no0001.txt` / `dos.txt`. Original pages now include a publisher-added Ruffle script and shared `activ.js`; native title captures recover both 7-key presentations. The index also mixes external destinations, hosted mirrors and a Wayback link. |
| [AutumnLeaves Flash list](https://autumnleaves.yukishigure.com/danoni.html) | [Higan](https://autumnleaves.yukishigure.com/higan/higan.html), [Destined Marionette](https://autumnleaves.yukishigure.com/_9/D_M.html) | Existing single-song source corpus. Higan is dated December 2006 with a documented 2008 remake; it has a validated native projection. The served SWF must not be labelled the unchanged 2006 original. Destined Marionette is acquired, with projection pending. |
| [MFV2 / おにぎり☆ぼっくす 5-key list](https://cw7.sakura.ne.jp/lst/ds0301/?keys=5), [publisher chart list](https://mfv2.sakura.ne.jp/do/ALL_List.php) | [Holy Moon](https://mfv2.sakura.ne.jp/do/main/?id=HolyMoon); later 7-key [FAKE800 legacy page](https://mfv2.sakura.ne.jp/do/__Flash/mfv2_005/danoni/DanOni/FAKE800/DO.FAKE800.html) | List contains original-era entries and modern HTML5 ports, plus multi-song selections such as TeamSaboMFV2. FAKE800's catalogue update is 2011-01-01; its SWF and observed `no7-31.txt` sidecar are local. `NewURL.js` reads a CSV and adds an HTML5-port link where available; it does not itself redirect the page. The indicated `/do/main/ruffle.js` currently returned 404. |
| [Vorhandensein work list](https://vorhandensein.sakura.ne.jp/danoni/) | [Dancing at Dusk](https://vorhandensein.sakura.ne.jp/danoni/295_dancing/), [カナリア](https://vorhandensein.sakura.ne.jp/danoni/366_canaria/), [ふしぎなひかり](https://vorhandensein.sakura.ne.jp/danoni/291_hikari/) | Native HTML5-era individual-song pages: 2021 5-key, 2024 7i, and 2020 mixed 9i/9A/9B respectively. Inline dos, shared CW v47.3.2 engine, ordinary MP3 paths and title/media/custom-script features. Select 9A/9B explicitly; the 9i chart stays outside the playable target. All four selected charts are captured and match offline; see the validation table below. |
| [chip creation work list](https://chipstar64.sakura.ne.jp/danoni.html) | [Jam with Pancake](https://chipstar64.sakura.ne.jp/danoni/danoni_JwP.html) | Individual-page HTML5 baseline already captured completely, with one 7-key chart and an offline browser reference. Provides another publisher rather than expanding only one engine/site. |

The primary [2006 festival home](https://cw7.sakura.ne.jp/event/7fes06/) records
August 19–21 and links its work list. Exact release, event, catalogue-update,
port and served-file revision dates are separate facts. HTTP Last-Modified and
an archive timestamp are not original publication dates.

For the 2003 boundary, the community's
[20th-anniversary introduction](https://vorhandensein.sakura.ne.jp/event/danoni20th/)
identifies the original O-Toro game. The
[site directory](https://cwt.mad.buttobi.net/danonidb/) links its `do.html` page.
A bounded Availability API lookup for that exact historical URL returned no
closest snapshot. The same happened for a 風見の地 homepage lookup. This is not
proof that every variation/capture is absent. No usable 2003 SWF was verified;
the concrete early-era corpus starts in 2004. The attempted live 風見の地 root
returned 403 and was not retried or circumvented.

## Findings that affect index input

**Discovery lists are not downloadable packs.** An author catalogue or event
is an ordered set of links across pages/hosts, potentially mixing engine
families, remakes, collaborations and removed works. Retain its name as a
collection label, not a guessed chart author, official release title or license.
A song retains its own music, credits, source URL and revision identity.

**Navigation can be operational data.** iso's three inspected pages test
`document.referrer` and redirect to the homepage when it lacks `flash.html`.
Acquisition used sandboxed Chrome visiting the actual list and clicking its
original anchors. The page's real referrer and subsequent browser request
context are recorded. No guard was removed, referrer spoofed or script evaluated
outside the browser. Keep this route in a discovery manifest so future capture
can reproduce it.

**Not every homepage is an HTML list.** iso's homepage embeds a small
navigation-only `index.swf`, whose strings include `flash.html`. It is not a
song. A static HTML walker would stop at this navigation movie. The reviewed
HTML list is the initial supported input; Flash navigation requires a separate
read-only link-inspection or browser-navigation adapter. Similarly, an event
landing page may link its actual work list rather than contain works itself.

**`noscript` can retain the original embed.** The 2006 festival pages use
`activ.js` to build the live embedding and preserve object/embed markup inside
`noscript`. Treat that markup as an observed SWF candidate, then confirm the
runtime destination. Deduplicate object `movie`, object `data` and embed `src`
when they identify the same movie, while retaining attributes and FlashVars.
Do not confuse Macromedia plugin installers, editor SWFs or navigation SWFs
with work content.

**Mode labels and song identity need more than link deduplication.** The MFV2
publisher list repeats work URLs with `scoreId` and sometimes doubly encoded
`musicName` values. Preserve semantic query bytes exactly. Grouping difficulties
or songs requires the publisher's reviewed router and runtime music mapping;
stripping query parameters can select another song. Identical SWF bytes can
also be a shared engine with different external dos/FlashVars. Deduplicate
resource acquisition by cache identity; do not deduplicate works by SWF hash.

**Legacy audio can be segmented.** Static inventory of the single-song sources
finds multiple SoundStreamHead tags, including 29 in FTHosyu, 34 in felys and
79 in RE:RAVE. Those counts include all nested sprites and do not mean that a
movie contains that many songs. Future audio extraction must identify active
stream/timeline relationships. A rule equating one stream header to one song,
or concatenating every SoundStreamBlock in the file, is insufficient.

## Wayback input and provenance

Use an explicit public snapshot link as a first-class input. Archive availability
is a separate acquisition concern from the legacy chart adapter. Public replay,
robots and site restrictions still apply; excluded, gated or withdrawn works
are not targets to circumvent. Keep the user's local-only corpus rule.

The observed RE:RAVE resource graph is a concrete test case:

| Resource | Requested snapshot | Actual final snapshot |
| --- | --- | --- |
| Work page | `20190314171455` | `20190314171455` |
| Embedded SWF (`oe_` link from the page) | `20190314171455` | `20190315015354` |
| External dos (`oe_` binding) | `20190314171455` | `20190331090421` |

The SWF is `a6fec2d1ac860d35d5481274fbed04b35c608049e30d8d2d30e01e3a5bcbbbae`;
the dos is `945926a605b118d9349d3b65a4d77b174043691ea13ef7686073fcc576c137f9`.
Both are byte-preserved locally. This is a mixed-capture restoration; no evidence
yet proves that those bytes form the identical 2005 release.

Internet Archive explicitly explains that missing linked resources can resolve
to a nearby capture or the live web. Its injected replay JS/Ruffle is current
replay infrastructure, not part of the historical author's engine. Store the
original logical URL, given replay URL, final replay URL, requested and actual
capture times, modifiers, redirects, retrieval date and body hash **per
resource**. Record live fallbacks; do not silently label them archived.
[Source: Internet Archive's replay guidance](https://help.archive.org/help/using-the-wayback-machine/).

A native movie should resolve its unchanged relative requests against the
original logical movie URL. The restricted navigator then serves exact cached
bindings from original URLs to captured bytes. This original/replay URL binding
was exercised by RE:RAVE's successful offline `dos.txt` request. Do not point native relative
loading at a partially rewritten Wayback path or replace URL strings inside
the SWF. Missing bindings stay explicit failures.

For later discovery of missing snapshots, use a bounded exact-URL Availability
or CDX query rather than a whole-host sweep. Availability's negative answer is
not a complete census. CDX supports exact matches, date ranges, MIME/status
filters and limits; save the lookup response and chosen record. Raw/identity
replay is useful for author HTML/sidecar bytes, but must stay within the same
public archive policy and retain separate evidence of the playable replay.
[Source: Internet Archive CDX API](https://github.com/internetarchive/wayback/blob/master/wayback-cdx-server/README.md).

Archive HTML introduces charset attributes on its injected scripts. RE:RAVE's
author HTML declares EUC-JP; selecting the first arbitrary `charset=` in the
rewritten page incorrectly picked a script's UTF-8. Honor response charset and
actual HTML encoding declarations, or use browser decoding; preserve raw bytes.
This matters for credits, song names and index link text.

## Local corpus and validation level

New sources live under `~/Music/DanOni/single-song-corpus-v1/`. Its root
`manifest.json` has schema `ddi-danoni-source-corpus/experimental-1`; each work
has `ddi-danoni-single-source/experimental-1`. This is a **source/adapter research
corpus**, not a new importer input or seven ready-to-play conversions.

| Work directory | Coverage | Current evidence |
| --- | --- | --- |
| `iso-fthosyu` | 2004, 5-key, early object-based author comment, embedded data/music | Original browser navigation, SWF header/tag inventory and native title capture |
| `iso-party4u` | 2004, 5-key, early embedded data/music | Same; no external dos request in the title capture |
| `iso-felys` | 2004, 5-key, external `dos.txt`, segmented streaming audio | First missing-sidecar run retained; subsequent cached-sidecar title run succeeds |
| `naoto-rerave` | 2005 event, 5-key LVUP/Hard, external dos, EUC-JP page, archived resources | Native title and dos load succeed; mixed March 2019 captures retained |
| `7fes06-happy-rainbow` | 2006, 7-key, publisher-hosted festival work, `no0001.txt` | Native title and external load succeed |
| `7fes06-decipher` | 2006, 7-key, another maker, `dos.txt` and freeze fields | Native title and external load succeed; gameplay hold semantics not projected |
| `mfv2-fake800` | 2011 catalogue date, 7-key, later ParaFla family, dynamically selected sidecar name | Native requests identify `no7-31.txt`; subsequent cached-sidecar title run succeeds |

All seven are AVM1 (no DoABC; native worker accepts them), with nominal 60 FPS
headers. That is not a verified chart/audio timebase. All seven have
`referenceValidated:false`. The native audits retain device-font fallback
warnings; raw state and rendering are source observations, not original Flash
visual fidelity. The existing Higan export remains the validated single-song
native reference; Kouhaku remains the separately validated collection case.

Three additional individual-song CW dumps live under `~/Music/DanOni/corpus/`:

| Directory | Selected charts / parsed data | Result |
| --- | --- | --- |
| `vh-dancing-at-dusk` | 0, 5-key: 546 taps, 51 holds, 4 speed events | Complete work; full offline verification passes |
| `vh-canaria-7i` | 0, 7i: 825 taps, 37 holds | Complete work; full offline verification passes |
| `vh-hikari-9ab` | 1, 9A: 74 taps/4 holds; 2, 9B: 163 taps/no holds | Both selected charts complete and verified offline; work partial because 9i chart 0 was deliberately not captured |

All four references come from browser-observed CW v47.3.2 state. The existing
local importer differential comparison now reports **13 references, zero
differences** across the full browser corpus. These are stronger references
than the seven preliminary legacy title captures above.

Each legacy source directory contains:

```text
manifest.json                        # URLs, date/mode hints, records, status/limits
inputs/title.json                    # restricted native run configuration
inputs/blobs/<sha256>                # exact list/page/movie/sidecar bodies
inputs/title-with-sidecar.json       # only where a missing binding was resolved
title-capture/{run,snapshot}.json     # initial execution and VM evidence
             /frame-{2,10,30}.ppm     # full viewport captures
             /stderr.txt
title-with-sidecar/...               # preferred second run for felys/FAKE800
```

`manifest.resources[]` retains the existing HTTP record fields, including robots,
request context, source retrieval date, original/final URL, redirects and hash;
`localPath` points to the work's exact copied blob. `dateHints` distinguishes
publisher publication, catalogue update and event dates. `keyModesHint` is a
selection hint until actual key definitions are validated. `preferredCapture`
selects the latest run; earlier failures remain preserved. The archive work
keeps original logical URLs in its native config and replay identities in its
HTTP records. The root inventory links the pre-existing Higan, Destined
Marionette and Straight Soul evidence without copying/refetching it.

Index/source acquisition logs, navigation evidence, API responses and resources
are under `single-song-corpus-v1/discovery/`. Source sidecars and title snapshots
are not `reference.json` chart oracles and must not be fed into the current
browser dump importer. No third-party bytes are committed.

## Browser-free normal path (maintainer direction)

The intended SWF pipeline is HTTP/cache → static HTML link/embed discovery →
restricted native Ruffle → export. Playwright is an optional discovery fallback,
not a native extraction dependency. Most inspected child URLs are already HTML
links, including server-rendered lists and `noscript` embeds. Known CSV wrappers
can be handled declaratively; ActionScript sidecar names can be observed by
native Ruffle. Browser navigation requirements remain recorded and site/request
restrictions must be respected. Arbitrary page JS executes only in a sandboxed
browser when that fallback is needed. Fresh CW runtime references still require
a browser as a separate oracle-capture step. Rebuilds can consume saved ones.
See [the export regeneration plan](danoni-exports.md) for the implementation
needed to replace the accumulated local export variants.

## Proposed index-input contract and implementation order

The existing browser CLI still captures **a work URL**. Index crawling is not
implemented by this survey. Add discovery above the current capture adapters,
so single-work, single-movie and collection cases share the lower-level tooling.

1. Accept a work-list/event URL, individual work page, explicit Wayback replay
   URL, or a local/remote SWF with page/base context. Local SWFs without resource
   context may be self-contained, otherwise report missing context.
2. Discover a bounded list first: one supplied index, declared work-list links
   or frame documents, then candidate work pages. Default to 5-key; offer the
   explicit supported-mode set. Use known publisher/index adapters for row
   labels and navigation, with conservative generic link candidates as fallback.
   Do not recursively crawl artist links, ranking endpoints, ads or the web.
3. Record candidates, duplicates, ports/remakes, skipped modes, failures and
   source page evidence. Dynamic indexes run only in sandboxed Chrome; acquired
   page JS is never evaluated in Node. Preserve the genuine browser route.
4. Capture the selected candidates through the existing CW or restricted native
   pipeline, with one shared paced cache. Unsupported legacy families remain
   acquired candidates rather than silently borrowing Kouhaku's projection.
5. Package independently verified songs under an explicit collection name.
   One song can have many difficulties; one SWF can have many songs; one index
   can link many SWFs. Keep these three levels separate. Artwork selection can
   consume verified song/collection metadata using the [artwork study](danoni-artwork.md).

Suggested commands are `discover INDEX --modes 5 --max-works 20`, followed by
`capture-list DISCOVERY --select ...`. These are **proposed**, not runnable CLI
options. A discovery pass should enumerate without downloading every song's
music; a capture pass acquires only selected work dependencies.

A proposed `ddi-danoni-discovery/experimental-1` JSON manifest should carry:

| Field | Required meaning |
| --- | --- |
| `input` | Given URL/path, detected kind, collection label and its evidence; optional archive-date selection |
| `policy` | Selected canonical modes, max work/page count, max discovery depth, pacing/cache and archive/live fallback policy |
| `pages[]` | Fetch record reference, role, original/final/logical URL, encoding, archive identity and retrieval/hash evidence |
| `works[].id` | Stable logical page/selection identity; content hashes remain revision evidence |
| `works[].route[]` | Ordered source-page links, frame or normal browser actions and final destinations; preserved semantic queries |
| `works[].modeHints` | Values plus source/confidence; unknown modes retained for capture classification |
| `works[].movieCandidates[]` | Embed/object/runtime request URL, logical/replay binding, dimensions, FlashVars and evidence path |
| `works[].relationships` | Original/port/remake/mirror/duplicate links with evidence, never guessed equivalence |
| `works[].status` | `candidate`, `acquired`, `title-captured`, `reference-validated`, `unsupported`, `unavailable` or `blocked`, plus diagnostics |
| `works[].dump` | Existing CW/native dump path/schema, selected chart IDs and validated media/reference status |

First implementation batch: iso's explicit `flash.html` list, the 2006 festival
work list, CW's per-author catalogue including NAOTO/MFV2, and a simple modern
publisher list. Add exact Wayback resource bindings with mixed-date reporting
before automatic archive fallback. Keep Flash-only navigation and arbitrary
JS-generated work lists as explicit later adapters. Expand native projections
from embedded 2004 5-key sources to external-dos sources and the 2006 7-key
family; use later supported-mode HTML5 references to protect the 7i/9A/9B path.
