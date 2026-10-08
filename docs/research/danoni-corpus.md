# DanOni publication survey and local corpus proposal

Research date: 2026-10-08. Status: **survey and proposed contract only**.
The dump tool has not been built, no reference parses have been captured in a
browser, and no playable corpus has been dumped. Candidate works below need
browser validation before inclusion. Work is isolated on `danoni-research` in
`../ditoditoinfinito-danoni`; the importer session owns the engine and PLAN.

## Findings and evidence

DanOni is distributed as individual websites, often with a shared engine
directory and per-work HTML, dos and music. Discovery indexes and events link
across those hosts. A work's publication year does not identify its engine:
authors can update shared scripts or load a versioned CDN build.

| Publishing route | Primary examples and observations |
|---|---|
| Personal sites | [Cross Walker](https://cw7.sakura.ne.jp/), [Vorhandensein](https://vorhandensein.sakura.ne.jp/danoni/), [chip creation](https://chipstar64.sakura.ne.jp/danoni.html), [気分転換にRPG](https://kt-rpg.sakura.ne.jp/danoni/main.htm). Numbered work pages/directories; shared JS and music; collaborations may link to another author's host. |
| Work lists and databases | [Work catalogue](https://cw7.sakura.ne.jp/lst/), [site list](https://cw7.sakura.ne.jp/danonidb/), [minor-key database](https://apoi108.sakura.ne.jp/danoni/ta/), [major-key database](https://apoi108.sakura.ne.jp/danoni/danoni_all_list/). Discovery services, not a single downloadable library. Registered counts are not a current-playability census. |
| Events | [V-Keys Festival '26](https://apoi108.sakura.ne.jp/event/nfes26/) (August 13–15, 2026), [2025 Winter](https://apoi108.sakura.ne.jp/event/nfes25_winter/), [20th-anniversary event](https://vorhandensein.sakura.ne.jp/event/danoni20th/). Release lists link out to works; the 2026 organizer offers hosting to authors without sites and explicitly prohibits excessive server load. |
| Collections inside a work | [7ikey Package](https://apoi108.sakura.ne.jp/danoni/7ikey_package/) has generated external dos and a custom song-selection UI, rather than one HTML document per song. |
| GitHub Pages | [a123's repository](https://github.com/a123-hp/danoni) advertises [a Pages site](https://a123-hp.github.io/danoni/), but its inspected landing page said “coming soon” and linked a 404 test page. No working Pages-hosted music work was verified in this survey. GitHub hosting of engine/tools should not be confused with verified hosting of playable works. |
| Flash-era sites | [気分転換にRPG's notice](https://kt-rpg.sakura.ne.jp/danoni/main.htm) says its Flash works were withdrawn on 2021-01-10. HTML5 ports and legacy Flash/Ruffle pages are different targets; Flash does not expose the CW JavaScript oracle. |

### Observed delivery patterns

The bounded sample is eight deliberately varied work pages: the five in the
first five rows below plus the three Cross Walker works in the sixth row.
**Six use inline dos; two use external dos.** This is not a random sample and
does not support ecosystem-wide percentages. Ordinary music-file references
occur in seven; one collection references encoded-music `.js` files. Five declare custom JS, four contain multiple charts, and three define custom
keys. Both inline/external and multi-chart/custom-script
packaging are recurring cases that the first tool should handle. The survey
inspected source, not successful end-to-end browser play.

| Work | Evidence from currently served source |
|---|---|
| [Summer Sky](https://vorhandensein.sakura.ne.jp/danoni/426_summersky/) | Inline dos, three charts (5/9B/23), MP3, two custom scripts; timed lyric PNGs. HTML credits include artwork/fonts in addition to music/maker fields. |
| [Jam with Pancake](https://chipstar64.sakura.ne.jp/danoni/danoni_JwP.html) | Inline dos, one 7-key chart, MP3, no explicit customjs header. Useful simple baseline. |
| [Dreadnought Course](https://chipstar64.sakura.ne.jp/danoni/danoni_7key_Dreadnought.html?stageNum=1) | Inline dos with five difficulty entries, course custom JS, WAV/MP3 references. `stageNum` makes the full URL significant. |
| [7ikey Package](https://apoi108.sakura.ne.jp/danoni/7ikey_package/) | `externalDos=dos.txt`; [dos.txt](https://apoi108.sakura.ne.jp/danoni/7ikey_package/dos.txt) builds a string in externalDosInit and assigns g_externalDos. Source contains 55 difficulty assignments and 23 music-title assignments (not yet a runtime chart count). Custom selection/package/scoring/ranking scripts; `.js` gameplay music references and `.m3u8` preview paths. Audio payloads were not downloaded/decoded during this survey. |
| [Tanoshii Dance Time](https://kt-rpg.sakura.ne.jp/danoni/html5/danoni/danoni_tanoshii.html) | External [score_tanoshii.txt](https://kt-rpg.sakura.ne.jp/danoni/html5/danoni/score_tanoshii.txt), explicit Shift_JIS decoding despite UTF-8 HTML, g_externalDos wrapper, MP3. Three charts: 9h/9h/custom eight-lane tanoshii; custom keys are dos headers. |
| Cross Walker: [Defeat,I Doll](https://cw7.sakura.ne.jp/danoni/2016/0273_DefeatIDoll.html), [Be Alike](https://cw7.sakura.ne.jp/danoni/2018/0313_BeAlike.html), [Twilight](https://cw7.sakura.ne.jp/danoni/2018/0316_Twilight.html) | All inline dos and MP3. Defeat has a 43-lane Tr pattern, key changes and lyrics; its current HTML has no customjs header. Be Alike and Twilight do load per-work custom JS for gameplay presentation; Twilight defines custom 7wi keys (15 lanes in its first pattern). |

The [custom-script repository](https://github.com/cwtickle/danoniplus-custom)
lists older implementations, including Defeat. Its README is useful discovery
evidence, but not proof that the live page still runs that script.

For actual data rewriting, the published
[key-change extension](https://github.com/suzme/danoni-key-change) modifies
`g_rootObj.difData` and lane arrays, can duplicate layouts or convert hold heads
to taps, and installs preloading callbacks. Its key definitions are supplied
through JS. This establishes the mechanism, but this survey did not verify a
live work deploying that extension. The package above independently provides
a live source example of generated dos. Custom key definitions may therefore
come from dos headers, shared settings JS, or extensions; capture resolved keys
regardless of their origin.

The upstream [authoring guide](https://github.com/cwtickle/danoniplus/wiki/HowtoMake)
also documents combined inline/external input, split files
(`externalDosDivide`), reused note suffixes (`externalDosLock`), charset and
URI/ampersand controls. Split-file deployment was not confirmed in the eight
pages. No login-gated work or deliberately obfuscated chart was encountered.
Conventional `.min.js` engine distribution is present; it is not evidence of
chart obfuscation. Broken links and removed works are confirmed obstacles.

### Engine versions actually served

| Publisher / source | Observation on 2026-10-08 |
|---|---|
| [Vorhandensein engine](https://vorhandensein.sakura.ne.jp/danoni/js/danoni_main.js) | Declares `Ver 47.3.2` |
| [chip creation engine](https://chipstar64.sakura.ne.jp/js/danoni_main.js) | Declares `Ver 48.4.1` |
| [apoi engine](https://apoi108.sakura.ne.jp/danoni/js/danoni_main.js) | Declares `Ver 50.2.0` |
| [気分転換にRPG engine](https://kt-rpg.sakura.ne.jp/danoni/html5/js/danoni_main.js) | Declares `Ver 50.2.0` |
| [Cross Walker loader](https://cw7.sakura.ne.jp/danoni/js/danoni_main.js) and [version.txt](https://cw7.sakura.ne.jp/danoni/js/version.txt) | Loader selects versioned jsDelivr/npm `danoniplus` main script; version file says `51.2.2`. Supports `?v=` override and local fallback. This is loader configuration, not a browser-confirmed CDN execution. |
| [Upstream pinned main script](https://github.com/cwtickle/danoniplus/blob/80c3c4776ed1727c10c78bcf6eb8963a4ed77523/js/danoni_main.js) | Declares `Ver 51.2.2`; upstream source reference, not a substitute for the deployed scripts above |

Local source evidence is cached under `/private/tmp/danoni-survey/`,
`/private/tmp/danoni-survey-packaging/` and
`/private/tmp/danoni-survey-engine/`. No music was downloaded and no fetched
script was executed. Direct publisher fetches checked robots and were paced;
the final dumper must retain that policy evidence per work, not rely on this
temporary survey cache.

## Proposed tool

Use `tools/danoni-dump/`: a small Node ESM command-line tool with
`playwright-core`, installed Chrome, versioned engine adapters, JSON Schemas,
and synthetic tests. Code is MIT under the existing `REUSE.toml` `**`
annotation. Any upstream sample dos fixtures need explicit upstream copyright
annotations; no upstream music. At implementation time resolve the latest
stable dependency versions and commit the tool's lockfile. No Rust dependency
or game-crate changes are needed.

Follow the installed-Chrome recipe in `.claude/skills/verify/SKILL.md`, with a
fresh ephemeral profile and Chrome's sandbox enabled. GPU flags are unnecessary
unless a particular work needs them. A real play click loads music. Pack scripts
execute only in the browser: never Node `eval`, `vm`, imports, or subprocesses.

The proposed commands (not implemented yet) are:

```sh
cd tools/danoni-dump
npm ci
node cli.mjs capture 'https://example.org/work.html' \
  --out "$HOME/Music/DanOni/work" --cache "$HOME/Music/DanOni/.cache"
node cli.mjs verify "$HOME/Music/DanOni/work" --offline
```

Capture all author charts by default, with explicit selection for an unusually
large work. Version 1 captures one selected pattern per author chart (default pattern 0),
while storing definitions for other patterns. Another captured pattern requires
a separate dump/run; this keeps the fixed per-chart paths unambiguous.
Keep one author chart separate from its alternate key patterns;
neither a different pattern nor a converted key mode silently becomes a new
source chart. Report inaccessible charts individually.

### Fetch discipline and completeness

- Check and cache robots.txt and linked applicable terms before work/resources
  on each origin, including redirects and cross-origin music. Respect stricter
  crawl delays. Default to one network request at a time per origin and at
  least one second between request starts. No login, paywall, challenge bypass,
  or retry through a different host to evade refusal. Unknown terms/robots
  failures are reported, not treated as positive permission.
- Route browser traffic through a persistent capture cache; serialize duplicate
  in-flight requests. Record full request and final URLs, redirects, status,
  MIME/charset, retrieval UTC time and SHA-256. Never request each asset again
  just to save it. Engine-added timestamp queries need an adapter-specific,
  recorded cache alias; never strip arbitrary query parameters. Reuse cached
  resources across charts and offline verification. Record range responses and
  require a complete media body before marking an audio file complete.
- Store all downloaded data beneath the corpus/cache or scratch directory.
  Preserve response bodies (after HTTP transfer/content decoding) byte for byte.
  Also retain the scripts containing base64 audio and the decoded media bytes;
  record that derivation and its MIME type without transcoding.
- Capture fetched dependencies plus finite URLs in parsed media events, CSS,
  skins and preloads. Separate engine assets from work media in the manifest.
  Browse cached resources under their original URLs using request interception,
  so relative paths, origin rules and `<base>` keep their meanings. An offline
  pass must fail on a cache miss rather than reach the network.
- A finite snapshot cannot promise arbitrary JS will never request something
  later. Record coverage (charts, patterns, scenes/time interval), unresolved
  URLs, console errors and runtime mutations. Label a dump `partial` when
  necessary. “Self-contained data for import” and “faithful offline replay of
  every custom animation” are separate validation results.

## Proposed dump layout: `ddi-danoni-dump/1`

This is a reviewable design, not yet a frozen interface. Implementation must
ship machine-readable JSON Schemas and verify them against actual captures
before the importer treats this version as stable.

```text
<work>/
  manifest.json                  # entry point, provenance, charts, assets, status
  resources.json                 # request URLs/redirects -> captured bodies
  blobs/sha256/<hex>              # content-addressed response bodies
  inputs/0000.dos.txt             # exact string supplied to dosConvert, UTF-8
  inputs/0000.string.json         # JSON string preserving exact JS code units
  inputs/index.json              # ordered calls, source, chart, parser flags
  charts/0000/
    effective-dos.json           # exact _dosObj at selected scoreConvert call
    keys.json                    # author definitions plus resolved key patterns
    reference.json               # normalized projection of engine results
    engine-snapshots.json        # invocation log, header, gauge, score, settings
  media/                         # usable decoded music/images, hash-based names
  capture.json                   # browser/tool/adapter versions and diagnostics
```

All paths are relative to the work directory; no `..`, absolute paths or
symlinks. Filenames never come directly from untrusted URL path components.
`manifest.json` is the only entry point: consumers must not guess chart or
music association from directory names. Hashes use lowercase SHA-256 hex;
dates use RFC 3339 UTC; all JSON/text files use UTF-8.

### Three distinct representations

1. **Inputs:** preserve each string passed to `dosConvert` in call order,
   after HTML entity/charset decoding or external JS execution, before splitting.
   Preserve original HTML/JS bodies too. The `.txt` is for inspection and parser
   input; `.string.json` is authoritative if JS contains lone UTF-16 surrogates
   that cannot round-trip as UTF-8. `inputs/index.json` records parser flags,
   source URL, chart ID, call sequence and both paths.
2. **Effective dos:** preserve the object actually supplied to the full
   `scoreConvert` invocation after inline/external merges and custom changes.
   It is not an “exact original dos text.” A synthesized `|key=value|` string
   loses duplicate keys, order and possibly escaping. Do not substitute one.
3. **Reference:** read the engine's computed results in Chrome. The tool only
   restructures its arrays and records their meaning; it must not independently
   parse dos to manufacture expected results.

### Manifest fields

Required top-level fields are `schema` (literal `ddi-danoni-dump/1`),
`page` (`requestedUrl`, `finalUrl`, `retrievedAt`), `engine` (`name`, declared
`version` or null, script URL/hash pairs, adapter ID), `credits`, `charts`,
`assets`, `status` (`complete|partial|unsupported|blocked`), and `diagnostics`.

`credits` retains raw `musicTitle`, localized alternatives and `tuning`, plus
engine-resolved titles/artists/URLs and makers per music/chart. These are
attribution, not inferred redistribution licences. Preserve additional credits
from the page (artwork, fonts, collaborators) with the original HTML resource.
`charts[]` contains
`chartId` (zero-based author selection index), `sourceScoreNumber` (engine's
resolved score number), `sourceSuffix` (string, often empty/2/3), `dosId`,
`musicId`, `keyMode`, `defaultPatternId`, relative paths to the four chart
files, and per-chart status. Keep `dosId`/score number separate: divided/locked
dos and explicit score numbering break an index-plus-one assumption.

`assets[]` contains a stable ID, role (`music|image|font|css|script|other`),
source URL(s), body hash/path, optional decoded-media path/hash, MIME type,
chart usage and extraction description. Missing resources have explicit
diagnostics rather than empty files. `resources.json` additionally retains
HTTP response metadata and cache aliases. No cookies or authorization secrets.

### Reference fields and units

One `reference.json` per chart and selected pattern has required fields:

| Field | Type and meaning |
|---|---|
| `schema` | Literal `ddi-danoni-reference/1` |
| `chartId`, `patternId` | Zero-based chart index and engine pattern identifier |
| `engineVersion`, `engineScriptHashes`, `adapterId` | Exact oracle identity |
| `captureStage`, `invocationId` | `scoreConvert-return-full` and selected call index |
| `fps` | Observed `g_fps`; initial adapter requires `60`. Explicit frame fields use engine frames, never milliseconds or beats; other effect payload units remain property-specific |
| `timing` | Raw author timing headers, resolved blank/start/end frames, playbackRate, header/user adjustment, preblankFrame, real/int/decimalAdjustment and any runtime-added padding; unavailable values explicitly null |
| `difData` | Raw header/row plus resolved keyLabel, difficulty label, creator, numeric difficulty, initialSpeed and raw resolved gauge-header values |
| `lanes` | Ordered array of `{index, chara, tapDataKey, holdDataKey, taps, holdEndpoints, holds, dummyTaps, dummyHoldEndpoints}` |
| `speed`, `boost` | Ordered arrays of `{frame:number, multiplier:number}` from engine pairs |
| `colors.legacy` | Ordered `{frame, target:number, color:string, all:boolean}` records from merged colorData |
| `colors.modern` | Map of engine part names to ordered `{frame, target:string, color:string, all:boolean, part:string}` records from ncolorData |
| `colors.dummyLegacy`, `colors.dummyModern` | Same shapes from dummyColorData/ncolorDummyData; preserve absent versus empty in snapshots |
| `effects` | Losslessly encoded native word/back/mask/style/motion/scroll/key-change arrays, with source property names; not silently omitted |
| `snapshotPath`, `diagnostics` | Pointer to full engine objects and explicit anomalies |

`taps` and `holdEndpoints` are exact engine number arrays. `holds` is a
convenience array of `[start,end]` pairs from adjacent endpoints, with an
unmatched endpoint reported instead of dropped. Keep duplicates and engine
ordering, including ties; do not fix malformed holds. Preserve dummy arrays
separately. Multiplier, adjustment and layout values may be fractional.

The full snapshot uses an explicitly tagged encoding for `undefined`,
non-finite numbers, sparse arrays and other non-JSON values. Plain
`JSON.stringify` alone is insufficient: it replaces NaN with null and erases
undefined properties. The encoder must avoid executing getters and mark
functions/DOM objects as opaque with a diagnostic. Normalized numeric fields
must be finite; any violation prevents a successful reference classification.

`keys.json` stores the engine's complete relevant `g_keyObj` entries and
resolved patterns, including lane `chara` names, key bindings, rotations/AA
glyph names, positions/divisions, color/shuffle/scroll/assist groups, aliases
and pattern IDs. Keep raw author definitions from dos and their source JS
resource IDs. Lane index is local to the chosen pattern, not universally L/D/U/R.
Changes made during gameplay require separate mutation coverage; a startup
key snapshot is not proof of a fixed layout.

### Where to observe the engine

At upstream commit `80c3c4776ed1727c10c78bcf6eb8963a4ed77523` (declares
v51.2.2), `scoreConvert` lives in `js/lib/dataLoader.js`. Many globals and
functions are lexical `const`/`let`; `window.g_scoreObj` is not a reliable
access path. Use browser execution-context bindings or CDP call-frame reads.

Proposed first adapter: CDP debugger breakpoints on hash-verified source
anchors, capture `dosConvert` input and the local `obj` at the **full**
`scoreConvert` return, then resume. No rewriting of fetched source. Distinguish
the chart-analysis call (`_scoreAnalyzeFlg=true`, early return) from the full
parse; analysis omits lyrics and later color merging. Retain every full call; the normalized reference selects the last full call
as a pure converter result, not as a claim that every returned field reaches
play unchanged. Preparation can call again with extra blank frames, then copy
only selected properties back: current `g_dataMinObj` excludes ncolor, scrollch
and keych data. The prepared state can therefore mix results from two calls.
Capture pre-`pushArrows` as the authoritative prepared-gameplay snapshot and
post-custom-hook state separately for audit.

`pushArrows` changes non-immediate color-event times into spawn times; taking
only `g_scoreObj` after gameplay starts produces the wrong parse oracle.
Version-specific source adapters are therefore necessary, including older
monolithic releases. Unsupported versions should retain raw resources but
must not claim a successful reference parse.

Use fresh settings, user adjustment 0, fade-in 0, shuffle/motion/reverse OFF,
normal scroll, author pattern 0 and Type0 colors; enable velocity, color,
background and arrow effects. Preserve author adjustment and playbackRate.
Record all effective options, locale and viewport, including options a work
forces. Autoplay may be used for subsequent resource traversal, separately
from the reference capture. In the current engine:

```text
realAdjustment = (userAdjustment + headerAdjustment) / playbackRate + preblankFrame
intAdjustment  = local-file audio ? round(realAdjustment) : floor(realAdjustment)
parsedFrame    = round((parseFloat(sourceFrame) - blankFrame) / playbackRate
                      + blankFrame + intAdjustment)
```

Source: [dataLoader.js, scoreConvert/prepareScoreData/pushArrows](https://github.com/cwtickle/danoniplus/blob/80c3c4776ed1727c10c78bcf6eb8963a4ed77523/js/lib/dataLoader.js).
Metadata and loading: [dosConverter.js](https://github.com/cwtickle/danoniplus/blob/80c3c4776ed1727c10c78bcf6eb8963a4ed77523/js/lib/dosConverter.js).

## Candidate corpus and importer handover

Proposed first local corpus, with **no dumps completed yet**:

| Candidate | Intended coverage and remaining check |
|---|---|
| Jam with Pancake | Simple inline 7-key/file-music baseline on v48; verify runtime notes/holds/events before tagging coverage |
| Tanoshii Dance Time | External JS-wrapped `.txt`, Shift-JIS, three charts, custom eight-lane keys and 9h |
| Summer Sky | 5/9B/23, multiple charts, older v47, custom JS, timed images/lyric media |
| Be Alike or Twilight | Per-work custom JS, presentation changes, nontrivial lane/hold field names; Twilight adds 7wi |
| Defeat,I Doll | Large custom layout, key-group changes and text lyrics; do not assume its historical custom JS is still deployed |
| Selected entries of 7ikey Package | External runtime-generated dos, collection enumeration, encoded music candidate and custom selector. Start with explicit subset; mark rest uncaptured to keep corpus small |
| Upstream MIT sample dos (separate from third-party works) | Deterministic holds, speed stops/changes, boost and legacy color fixture coverage already documented in B1. Copy only attributed chart text if fixtures are later added; no music |

Dreadnought Course is a second-stage candidate for query-dependent stages and
multiple music resources. Find a browser-confirmed deployed key-rewriting
extension and a divided-dos work before claiming those corpus coverage tags.
Likewise confirm modern ncolor and textual word_data in the captured state;
source filenames or a visually changing page are not sufficient evidence.

The importer session can plan around `manifest.json` and the ordered input,
effective-object and reference separation now; field names remain proposed
until browser captures validate the adapter. Native differential tests should
consume local dumps through an opt-in environment variable, never package
third-party content into fixtures or CI artifacts.

Keep source frames and transformed oracle frames distinct. At 75 BPM and
48 ticks/beat, an integer 60 Hz source frame maps exactly to one tick. This
follows PLAN decision 4; the earlier Part C claim that conversion to beats is
necessarily lossy is superseded for this repository. Fractional source values
and author playback-rate rounding require explicit tests, not a blanket
integer-frame assumption.

Corrections to carry into the importer: tap/hold tokens use `parseFloat`, not
the effect expression evaluator; legacy speed_change wins if both spellings
are present in the current implementation; boost selection uses the note's
arrival frame, not spawn time; colors have different times before and after
play preparation. See the dated additions in `formats-ffr-danoni.md`.

## Main risks and proposed acceptance checks

1. **Oracle stage and version drift:** validate adapters on actual served
   scripts, record hashes, compare captured note counts with the engine's chart
   details, and reject an analysis-only or post-mutation snapshot.
2. **Runtime custom data:** retain source inputs, effective objects and hook
   snapshots. Detect differences and report whether a static importer can
   represent them. Arbitrary gameplay code cannot be reduced to dos in general.
3. **Incomplete multi-chart/media traversal:** visit every author chart,
   resolve per-chart audio and divided dos, then replay from cache with network
   disabled. Report chart-level failures and late media cache misses.
4. **Non-reproducibility:** record options, locale, viewport, script hashes and
   capture stages. Repeat offline and compare parsed arrays; report randomness
   or time-dependent differences without silently seeding a modified work.
5. **Historical availability and rights:** a public list entry is not proof
   that its game still runs or that its assets may be redistributed. Honour
   access rules, stop on removed/Flash-only/protected works, and keep media local.

Before declaring version 1 stable, exercise inline+external precedence,
split/locked chart numbering, encoding/ampersand flags, holds, speed and boost,
both color forms, lyrics, custom keys, actual data-mutating custom JS, MP3 and
base64 music, and at least two deployed engine generations. Include an early first note
that triggers automatic preblank with modern colors, scroll and key events. Use small
synthetic or attributed MIT chart fixtures for automated tool tests; real
works remain opt-in local verification only.
