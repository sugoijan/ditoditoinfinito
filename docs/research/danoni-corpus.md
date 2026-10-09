# DanOni publication survey, dump contract and importer handover

Research date: 2026-10-08. The dump tool is implemented in
[`tools/danoni-dump/`](../../tools/danoni-dump/README.md). Its Chrome integration
test captures synthetic charts and replays them with the fixture origin shut
down. Nine local HTML5 works provide thirteen browser-derived chart references; all
thirteen reproduce offline. Jam with Pancake and two new individual-song dumps
pass the full startup/assets check; a third new mixed-mode work passes for both
selected supported charts and remains partial because 9i was not selected. See
the inventories below.
Work is isolated on `danoni-research`; the importer session owns the game crates
and `docs/PLAN.md`.

Follow-up direction: prioritize 5-key archives while retaining native 7/9-key
definitions. The [legacy extraction work plan](danoni-legacy-plan.md) describes
Japanese discovery, HTML5 corpus expansion and a separate native headless Ruffle
AVM1 feasibility study. It also specifies the proposed CW dos export alongside
preserved originals and runtime references. The goal is portable static charts;
arbitrary custom-engine gameplay is out of scope. A separate
[native prototype](../../tools/danoni-dump/ruffle/README.md) now proves AVM1
inspection and dos reconstruction, including Higan and the six-chart
[2004 Kouhaku collection](danoni-kouhaku.md).
The [native handover](danoni-native.md) specifies that pilot's observed timing
and export; broader legacy conversion remains experimental and does not change
the browser dumper's `/1` contract.

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
packaging are recurring cases, rather than exceptional export paths. The survey
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
`/private/tmp/danoni-survey-engine/`. During that source-only survey, no music
was downloaded and no fetched script was executed. Direct publisher fetches
checked robots and were paced; the implemented dumper retains its own policy
evidence per work rather than relying on this temporary survey cache.

## Implemented tool

The [Node ESM tool](../../tools/danoni-dump/README.md) uses `playwright-core`,
installed Chrome, CDP observation, versioned JSON Schemas and synthetic tests.
Node 22 or later is required. Source, pinned dependency lockfile and original
synthetic fixtures are MIT under `REUSE.toml`; no third-party work/music is
committed. No Rust dependency or game-crate changes are needed.

Each chart uses a fresh Chrome context with sandbox enabled, `en-US`, 1280×960,
and autoplay permission. A play-button click loads music. Service workers,
downloads and embedded documents are excluded. Pack scripts execute only in
Chrome: never Node `eval`, `vm`, imports, or subprocesses. The tool observes
browser-decoded engine source without modifying it.

The commands are:

```sh
cd tools/danoni-dump
npm ci
node cli.mjs capture 'https://example.org/work.html' \
  --out "$HOME/Music/DanOni/work" --cache "$HOME/Music/DanOni/.cache" \
  --terms-notes 'Public work; reviewed linked terms on YYYY-MM-DD.'
node cli.mjs verify "$HOME/Music/DanOni/work" --offline
npm test
```

Replace the example URL, preserving meaningful queries such as `dos`, `scoreId`,
`stageNum`, `key` and `v`. Output must be new or empty and outside a Git checkout;
verification rejects symlinks. The cache defaults to the output directory.
`--charts 0,2` selects zero-based author indices; without it the runner enumerates
`g_headerObj.keyLabels`. `--timeout-ms 120000` sets the browser operation timeout
in milliseconds. Progress goes to stderr, JSON to stdout. Exit 0 means no failed
selected chart/replay; 2 means an incomplete selected chart or failed replay;
1 means an invocation/fatal error. A chart subset can exit 0 while the overall
manifest remains `partial`.

Version 1 captures **one active pattern per author chart**, normally author
pattern 0 with fresh settings, and stores other available definitions. There is
no CLI pattern selection/enumeration option. Custom code can force a pattern:
read `activePattern` rather than assuming `_0`. Author chart, alternate pattern,
and converted key mode remain distinct concepts.

### Fetch discipline and completeness

- GET requests run anonymously as `DdiDanoniDump`, with `Accept: */*` and
  gzip/deflate/Brotli support for direct fetches. Browser-triggered requests
  forward the browser's actual Accept, Origin and Referer values; these are not
  invented overrides. Cookies and Authorization are not forwarded; Set-Cookie
  is not retained. Non-GET requests and WebSockets are
  blocked. Private/reserved network addresses are rejected by the public CLI;
  the programmatic exception exists for synthetic local tests only. Sites
  needing credentials can fail; there is no bypass option. Captured request
  context is checked against `Vary` before cache reuse; a conflicting or
  unsupported variant fails rather than refetching a different representation.
- Robots is checked per origin, including redirects. HTTP 200 is parsed; 404
  means no robots rules; other failures stop that origin's requests. Requests
  are serialized per origin and paced at least one second apart, respecting a
  stricter crawl delay. Duplicate requests share a fetch; persisted bodies and
  failures prevent repeated downloads/retries. Keep one writer per cache.
- **Terms need operator review.** `--terms-notes` records that review and
  `capture.json` lists terms-like page links. The tool does not automatically
  fetch, interpret or approve all linked terms. Public playability, robots
  permission and the engine's MIT licence do not grant asset redistribution
  rights. Do not bypass removed content, protections or refusals.
- All downloads stay under output/cache or scratch. Response bodies are saved
  byte-identically after HTTP content decoding, without transcoding. The
  allowlisted headers retain content type/charset, redirects/cache validators
  and selected CORS/CSP/origin-policy fields, not a complete HTTP conversation.
  HTTP 206 is rejected; range fragments are not reconstructed. The current
  decoded-body limit is 128 MiB and network request deadline is 30 seconds.
  Base64 music keeps both its script response and decoded bytes.
- Only reviewed-engine timestamp-only queries and the exact version probe of a
  hash-reviewed loader are explicitly aliased; semantic queries remain intact.
  Previously captured timestamp variants can be reused only when their recorded
  bodies and response metadata agree; conflicting variants fail closed. Replay intercepts original URLs so relative paths and
  browser origins retain their meanings. Offline misses fail without fetching.
- Besides startup requests, the runner collects current DOM images, preload
  links, accessible CSS rule URLs, and `src`/CSS URLs in parsed `htmlText` effect
  data. Cross-origin CSS inaccessible through CSSOM is saved as a stylesheet;
  **unrequested nested assets are not exhaustively walked**. Embedded frames,
  arbitrary future JS URLs, later scenes, course transitions and streaming
  preview traversal remain outside guaranteed coverage. Review diagnostics;
  import-data completeness does not promise every custom animation replays.

## Dump contract: `ddi-danoni-dump/1`

The following layout is emitted by the implemented tool. The linked schemas,
semantic reference validation and recorded capture stages define its interface;
real-work coverage is tracked separately from contract support.

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
  media/<sha256>.<ext>            # decoded/copied gameplay audio; images stay in blobs
  capture.json                   # browser/tool/adapter versions and diagnostics
  verification.json              # added by verify --offline
```

Contract paths are relative; no `..`, absolute paths or backslashes. Dump
verification rejects symlinks. Filenames never come from URL path components.
`manifest.json` is the only entry point: consumers must not guess chart or
music association from directory names. Hashes use lowercase SHA-256 hex;
dates use RFC 3339 UTC; all JSON/text files use UTF-8.

The cache also writes `index.json` (`ddi-danoni-cache/1`), possibly in the work
folder when the cache defaults there. It is not an importer input. The dump's
`resources.json` and bodies suffice for replay without that original cache.
Verification writes a scratch replay directory and reports its path.

| Artifact | Schema/version and authoritative definition |
|---|---|
| `manifest.json` | `ddi-danoni-dump/1`; [manifest schema](../../tools/danoni-dump/schemas/manifest.schema.json) |
| `inputs/index.json` | `ddi-danoni-inputs/1`; [inputs schema](../../tools/danoni-dump/schemas/inputs.schema.json) |
| `effective-dos.json` | `ddi-danoni-effective-dos/1`; [effective-dos schema](../../tools/danoni-dump/schemas/effective-dos.schema.json) |
| `keys.json` | `ddi-danoni-keys/1`; [keys schema](../../tools/danoni-dump/schemas/keys.schema.json) |
| `reference.json` | `ddi-danoni-reference/1`; [reference schema](../../tools/danoni-dump/schemas/reference.schema.json) |
| Nested native snapshots | `ddi-danoni-snapshot/1`; [snapshot schema](../../tools/danoni-dump/schemas/snapshot.schema.json) and [encoder/decoder/projection](../../tools/danoni-dump/serialization.mjs) |
| `engine-snapshots.json` | `ddi-danoni-invocations/1`, `{schema,snapshots:[<tagged snapshot>,...]}`; [dump.mjs](../../tools/danoni-dump/dump.mjs) |
| `capture.json`, `verification.json` | `ddi-danoni-capture/1`, `ddi-danoni-verification/1`; [dump.mjs](../../tools/danoni-dump/dump.mjs) |
| `resources.json` | `ddi-danoni-resources/1`, `{schema,resources,aliases,diagnostics,offlineMisses}`; [network.mjs](../../tools/danoni-dump/network.mjs) |

The last three rows have implementation-defined wrappers, not standalone JSON
Schemas. A structurally valid reference with an error diagnostic still fails
the semantic `validateReference` check.

### Three distinct representations

1. **Inputs:** preserve each string passed to `dosConvert` in call order,
   after HTML entity/charset decoding or external JS execution, before splitting.
   Preserve original HTML/JS bodies too. The `.txt` is for inspection and parser
   input; `.string.json` is authoritative if JS contains lone UTF-16 surrogates
   that cannot round-trip as UTF-8. `inputs/index.json` records parser flags,
   caller locations, URLs, chart IDs, call sequence and both paths.
2. **Effective dos:** preserve the object actually supplied to the full
   `scoreConvert` invocation after inline/external merges and custom changes.
   It is not an “exact original dos text.” A synthesized `|key=value|` string
   loses duplicate keys, order and possibly escaping. Do not substitute one.
3. **Reference:** read the engine's computed results in Chrome. The tool only
   restructures its arrays and records their meaning; it must not independently
   parse dos to manufacture expected results.

`sequence` restarts within each fresh chart capture. `captureChartId` is the
requested author index; `chartId` is the engine selection at the input call and
can still be 0 during startup for a later requested chart. Group records by
`captureChartId`, not observed `chartId`. The two flags are
`enableAmpersandSplit` and `enableDecodeURI`. External `.txt` can contain
executable JS: never parse its response as plain dos or execute it in an importer.

`effective-dos.json` has a `fields` map of string-valued properties and a tagged
`native` projection of the whole converter input. Non-string fields are retained
only in `native` and make the chart partial. `keys.json.definitions` and
`reference.effects` are similarly projected native values, not new independent
parses of the text.

`manifest.engine.scriptHashes` and `reference.engineScriptHashes` hash the
**browser-decoded source string** read through CDP, encoded as UTF-8 for hashing.
`resources[].sha256` and `assets[].sha256` hash saved response-body bytes. These
can differ because of BOM removal or charset decoding. A version label alone
never selects an adapter. The [registry](../../tools/danoni-dump/engine-hashes.json)
contains reviewed sources. `--trust-engine SHA256` may be repeated only for
sources explicitly inspected for this adapter; it is not a blanket assertion
that every build declaring the same version is compatible.

### Manifest fields

Required top-level fields are `schema` (literal `ddi-danoni-dump/1`),
`page` (`requestedUrl`, `finalUrl`, `retrievedAt`), `engine` (`name`, declared
`version` or null, script URL/hash pairs, adapter ID), `credits`, `charts`,
`assets`, `status` (`complete|partial|unsupported|blocked`), and `diagnostics`.
`capturePath` and `resourcesPath` link the supporting files. Null chart artifact
paths mean that capture did not produce the corresponding result; a partial
chart can still have an invocation log.

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
Only selected gameplay audio is explicitly mapped to chart IDs; an empty
image/script chart-ID list means usage was not resolved, not that it is unused.
Images remain in `blobs/sha256`; audio under `media/` is either decoded from its
captured music script or copied byte-identically from an audio response.
`resources[]` includes requested/final URLs, status, allowlisted headers,
MIME/charset, body path/hash/length, retrieval time, redirect hops, robots-policy
references, roles and cache origin.

### Reference fields and units

One `reference.json` per chart and selected pattern has required fields:

| Field | Type and meaning |
|---|---|
| `schema` | Literal `ddi-danoni-reference/1` |
| `chartId`, `patternId` | Zero-based chart index and engine pattern identifier |
| `engineVersion`, `engineScriptHashes`, `adapterId` | Exact oracle identity; current adapter is `cw-lexical-cdp/1` |
| `captureStage`, `invocationId` | `scoreConvert-return-full` and index in this capture's complete snapshot log |
| `fps` | Observed `g_fps`; initial adapter requires `60`. Explicit frame fields use engine frames, never milliseconds or beats; other effect payload units remain property-specific |
| `timing` | Raw author timing headers, resolved blank/start/end frames, playbackRate, header/user adjustment, preblankFrame, real/int/decimalAdjustment and any runtime-added padding; unavailable values explicitly null |
| `difData` | Raw header/row plus resolved keyLabel, difficulty label, creator, numeric difficulty, initialSpeed and raw resolved gauge-header values |
| `lanes` | Ordered array of `{index, chara, tapDataKey, holdDataKey, taps, holdEndpoints, holds, dummyTaps, dummyHoldEndpoints}` |
| `speed`, `boost` | Ordered arrays of `{frame:number, multiplier:number}` from engine pairs |
| `colors.legacy` | Ordered `{frame, target:number, color:string, all:boolean}` records from merged colorData |
| `colors.modern` | Map of engine part names to ordered `{frame, target:string, color:string, all:boolean, part:string}` records from ncolorData |
| `colors.dummyLegacy`, `colors.dummyModern` | Same shapes from dummyColorData/ncolorDummyData; preserve absent versus empty in snapshots |
| `effects` | Tagged native projection of remaining score properties, including word/back/mask/style/motion/scroll/key-change arrays, with source property names |
| `snapshotPath`, `diagnostics` | Snapshot path relative to the reference's chart directory; anomalies with severity/code/path/message |

`taps` and `holdEndpoints` are exact engine number arrays. `holds` is a
convenience array of `[start,end]` pairs from adjacent endpoints, with an
unmatched endpoint reported instead of dropped. Keep duplicates and engine
ordering, including ties; do not fix malformed holds. Preserve dummy arrays
separately. Multiplier, adjustment and layout values may be fractional.
Speed/boost multipliers are unitless. Gauge headers retain engine meanings
(`Border`, `Recovery`, `Damage`, `Init`, including `Border=x`), not normalized
life deltas. `effects` retains property-specific units and sparse indexing;
not every numeric payload member is a frame. Optional absent colors become
empty normalized collections; snapshots retain absent-versus-empty distinctions.
`runtimeAddedPadding` currently remains null: the adapter records each call's
`preblankFrame` rather than deriving this reserved extra field.

Each tagged snapshot has `{schema,root,diagnostics}`. Primitive JSON values are
literal; tags encode undefined, special numbers, bigint, opaque values,
references, objects and arrays. Objects contain `id`, `nullPrototype` and ordered
`[key,node]` entries. Arrays contain `id`, `length` and entries with string
indices, preserving holes. Reference nodes point to previously encoded IDs.
See the linked schema for exact tags; this is not ordinary JSON with metadata.

The encoder avoids getters and `toJSON`; functions, symbols and exotic/DOM
objects are opaque with diagnostics. `projectSnapshot` extracts subtrees without
decoding/re-encoding opaque values; referenced nodes outside the subtree are
included and renumbered. Diagnostic paths retain their original snapshot paths.
Nonfinite normalized numbers, missing required lane/gauge fields or incomplete
event tuples invalidate a reference. Unrelated opaque extension functions are
warnings; executable behavior is never reconstructed.

`keys.json` has `keyMode`, `activePattern`, resolved `lanes`, and tagged
`definitions` projecting captured `g_keyObj`: available patterns, bindings,
shape/rotation, position/division, color/shuffle/scroll groups and extension
fields. Raw dos definitions remain in effective fields; original JS remains in
resources, without per-definition source mapping. `tapDataKey`/`holdDataKey`
resolve actual engine aliases, including `sfrzLeft_data` and `frzLdia_data`.
Lane index belongs to this pattern, not a universal arrow order. Definitions do
not enumerate playable variants or prove the layout stays fixed during play.

### Three engine observation stages

At upstream commit `80c3c4776ed1727c10c78bcf6eb8963a4ed77523` (declares
v51.2.2), `scoreConvert` lives in `js/lib/dataLoader.js`. Many globals and
functions are lexical `const`/`let`; `window.g_scoreObj` is not a reliable
access path. The tool uses CDP call-frame reads.

The [adapter](../../tools/danoni-dump/adapter.mjs) parses reviewed source syntax
with Acorn and installs CDP breakpoints without rewriting the engine. It captures
`dosConvert` input separately, then these three snapshot stages:

| Stage | Observation point and use |
|---|---|
| `scoreConvert-return-full` | Final top-level return before it executes: local result plus input/header/state/key/gauge. Analysis-only early returns are excluded. The last full call **for the requested chart** becomes the normalized reference. |
| `prepared` | Immediately before `pushArrows(g_scoreObj,...)`, after preblank handling but before spawn/event scheduling mutates the data. |
| `runtime` | Entry to `mainInit`, before its first statement. Can include earlier loading-hook changes; **not after main-init custom hooks**, not a post-frame snapshot, and not a mutation trace. |

All full calls remain in the invocation log. Preblank reconversion can copy
only selected arrays back into `g_scoreObj`; the reviewed v51 `g_dataMinObj`
excludes ncolor, scrollch and keych. Prepared gameplay can therefore mix two
converter passes. The selected reference describes a converter result, not a
claim that every returned field reaches gameplay unchanged.

`pushArrows` changes non-immediate color-event times into spawn times; taking
only `g_scoreObj` after gameplay starts produces the wrong parse oracle.
Version-specific source adapters are therefore necessary, including older
monolithic releases. Unsupported sources retain raw resources and diagnostics
rather than claiming a successful reference parse.

Use fresh settings, user adjustment 0, fade-in 0, shuffle/motion/reverse OFF,
normal scroll and author colors; enable velocity, color,
background and arrow effects. Preserve author adjustment and playbackRate.
Record all effective options, locale and viewport, including options a work
forces. The current tool does not autoplay through later scenes. In the reviewed
engines:

```text
realAdjustment = (userAdjustment + headerAdjustment) / playbackRate + preblankFrame
intAdjustment  = local-file audio ? round(realAdjustment) : floor(realAdjustment)
parsedFrame    = round((parseFloat(sourceFrame) - blankFrame) / playbackRate
                      + blankFrame + intAdjustment)
```

Source: [dataLoader.js, scoreConvert/prepareScoreData/pushArrows](https://github.com/cwtickle/danoniplus/blob/80c3c4776ed1727c10c78bcf6eb8963a4ed77523/js/lib/dataLoader.js).
Metadata and loading: [dosConverter.js](https://github.com/cwtickle/danoniplus/blob/80c3c4776ed1727c10c78bcf6eb8963a4ed77523/js/lib/dosConverter.js).
The tool navigates HTTP(S) origins, so the local-file branch normally does not
apply. Older deployments keep equivalent functions in a monolithic main script.

## Corpus inventory

The local root is `/Users/jan/Music/DanOni/corpus/` (not in Git). Captured and
verified on 2026-10-08 with installed Chrome; exact browser version, source hashes,
retrieval times, credits and resource policies live in each dump. The following
counts come from validated `reference.json`, not an independent dos parser.
Chart IDs are zero-based. All **nine references match a fresh offline browser
conversion**; that result is separate from successful replay of every required
startup asset.

| Directory / public work | Captured charts and engine | Observed coverage | Capture / offline result |
|---|---|---|---|
| `jam-with-pancake` — [Jam with Pancake](https://chipstar64.sakura.ne.jp/danoni/danoni_JwP.html) | 0; 7 lanes; v48.4.1 | Inline dos, ordinary MP3, 1,160 taps, 140 holds, 16 speed events | Complete; offline verification passes |
| `tanoshii` — [Tanoshii Dance Time](https://kt-rpg.sakura.ne.jp/danoni/html5/danoni/danoni_tanoshii.html) | All 0–2; 9h/9h/custom 8-lane `tanoshii`; v50.2.0 | External JS-wrapped Shift_JIS dos, multiple charts and custom keys; 270/423/256 taps | All three references match offline; partial because music returned HTTP 403 |
| `7ikey-package-25-menu` — [7ikey Package](https://apoi108.sakura.ne.jp/danoni/7ikey_package/) | 24 (source suffix 25), of 55 enumerated charts; 7i; v50.2.0 | Runtime-generated external dos, custom menu/hooks, decoded MP3 from `shinseiji-bugi.js`; 1,475 taps, 54 holds, 7 speed and 3 boost events; 308 modern color events; parsed background/mask effects | Reference matches offline; partial due page errors, non-string effective field and unavailable preview/font resources; other 54 charts not captured |
| `twilight` — [Twilight](https://cw7.sakura.ne.jp/danoni/2018/0316_Twilight.html) | 0; custom 15-lane `7wi`; v49.1.1 fallback | Inline dos, custom JS, MP3, 1,250 taps, 38 holds, 2 speed events; 47 populated lyric frames | Reference matches offline; partial: shared arrow image missing, CDN unavailable |
| `defeat-i-doll` — [Defeat,I Doll](https://cw7.sakura.ne.jp/danoni/2016/0273_DefeatIDoll.html) | 0; custom 43-lane `Tr`; v49.1.1 fallback | Inline dos, MP3, 762 taps, 23 holds, 2 speed events; 153 populated lyric frames and 47 key-group frame entries | Reference matches offline; partial: shared arrow image missing, CDN unavailable |
| `straight-soul-html5` — [Straight Soul -Web Style-](https://cw7.sakura.ne.jp/danoni/2009/0169_StraightSoul.html) | All 0–1; 5-key Normal / 7-key Hard; v49.1.1 fallback | Inline dos, MP3; 1,144/1,515 taps and 9/9 holds; Normal has 5 speed and 9 boost events; custom/key-change scripts delivered, conversion options not activated | Both references and chart startup reproduce offline; work partial due excluded statistics iframe and recorded unavailable bootstrap resources |

The package's parsed boosts are `(6285, -0.5)`, `(6310, -1)`, `(6490, 1)`;
these differ from source frames by the recorded adjustment. Color counts are
8 Arrow + 100 Normal + 100 NormalBar + 50 Hit + 50 HitBar events. Its extra
`musicTitles` object remains intact in the tagged native effective input; the
string-only convenience map deliberately does not pretend to contain it.
Tanoshii's timer-header references were regenerated from the original native
snapshots after numeric-string handling was corrected; no additional fetch was
needed, and its manifest records that reprojection.

The initial Tanoshii and Summer Sky captures preceded browser Referer/Origin/
Accept forwarding. Their music responses were HTTP 403; that does not establish
that an ordinary browser cannot play them. Those responses were retained and
not retried. The tool now forwards actual browser headers, without fabricated
headers or credentials. A Cross Walker probe stopped on an unreviewed engine
before an in-flight arrow-image request completed; that missing response remains
visible in the later partial dumps. Never-started queued requests can subsequently
run, but dispatched failed requests are not retried. The CDN's robots endpoint
returned HTTP 400, so the tool blocked it; Cross Walker's own loader then selected
its local v49.1.1 engine. This was normal site fallback behavior.

Additional diagnostic directories are retained locally: `summer-sky` (three
charts, music 403 before a full parse), `dreadnought-stage-1` (`?stageNum=1`,
chart 0; course hook reports missing `getStrLength`, music 403),
`be-alike-probe` (initial unreviewed fallback-engine detection), and
`7ikey-package-25` (initial custom-menu discovery). These are not usable
reference corpus entries. Earlier scratch attempts are outside this inventory.
No failures were hidden by substituting synthetic assets or another engine.

The loaded Twilight resources include the
[publisher's key-change extension](https://cw7.sakura.ne.jp/danoni/js/key_change.js),
confirming live delivery of a key-rewriting script. This capture did not activate
its `key`/`double` URL options, so it does not validate those conversion branches.
Divided dos, alternate key patterns and later course stages still need separate
corpus coverage. Current committed fixtures are entirely original synthetic
content; no third-party charts, music, scripts or images were committed.

## Importer handover and acceptance limits

Use `manifest.json` as the entry point, then validate each chart's status and
reference contract. For simple inputs, differential tests can merge the ordered
strings with captured parser flags. For generated/rewritten works,
`effective-dos.json` supplies the static converter input; importing it must not
execute captured scripts. Resolve lanes through `keys.json`, compare at the
recorded timing settings, and investigate prepared/runtime differences
separately instead of modifying expected notes to hide them. Native differential
tests should take an opt-in local corpus path; never package third-party dumps
into fixtures or CI artifacts.

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

`complete` is a bounded import-data result, not a licence decision or a promise
of faithful full-song replay. The overall manifest requires all enumerated author
charts complete and no recorded network limitations. Chart completeness requires
input/prepared captures, a valid full reference and complete mapped audio; page
or adapter failures make it partial. `capture.json` reports narrower browser
startup status; use manifest chart status for importer acceptance.

`verify --offline` hashes saved assets, starts fresh Chrome contexts using only
the dump, and compares newly normalized last full returns to saved references.
`verification.comparisons` records `{chartId, referenceMatched}` even when a
replay is incomplete. Missing cache entries or changed parses fail the overall
`passed` result. It does not compare every native
input/header/DOM snapshot, visit every later scene, or prove every hook
deterministic. `verification.json` and `capture.coverage.offlineVerified` record
its result; `fullGameplayReplay` remains false. The known music-select extension
is supported through its `lblMusicNumBarSlider` input and Enter handler, using
the chart's resolved music index so its normal hooks run. Flash, other arbitrary
custom selection controls, alternate-pattern enumeration, multi-stage courses and per-frame
JavaScript-to-static-event conversion remain outside supported scope.

### Importer integration after the rebase (2026-10-09)

Local `main` at `36e57b1` implements CW import and reads the existing
`ddi-danoni-dump/1` manifest/effective-dos contract through
[`library/src/danoni.rs`](../../library/src/danoni.rs). This research branch is
rebased onto that implementation; the dump schemas are unchanged. The game's
supported modes are **5, 7, 7i, 9A and 9B**. Other modes and work-defined or
redefined layouts are reported as unsupported. Retaining those works in the
research corpus still tests extraction and layout interpretation; it is not a
promise that the game plays them. See PLAN decisions 26–27 and
[`PLAYED_DANONI_MODES`](../../chart/src/layout/mod.rs).

The importer now has an opt-in local comparison:

```sh
DDI_DANONI_CORPUS="$HOME/Music/DanOni/corpus" cargo test -p ddi-chart \
  --test danoni_corpus -- --nocapture
```

[`danoni_corpus.rs`](../../chart/tests/danoni_corpus.rs) compares lane names,
tap/hold frames, speed/boost pairs and integer timing adjustment, and checks
whether each chart imports according to the supported-mode policy. It does not
compare colours, lyrics, audio playback, arbitrary custom hooks or legacy export
manifests. A comparison can use a reference from a partial dump; passing it does
not upgrade that dump's asset or replay status. The legacy pilot keeps its
separate [CW round-trip evidence](danoni-native.md).

After rebasing onto `36e57b1`, this test compared all nine captured references
with zero differences. Six chart entries in earlier incomplete acquisition
attempts were skipped because they had no effective dos or reference. Unsupported
modes still participate in the data comparison and are expected to be rejected
by the game import step.

The importer currently plays at the music's original rate; source playback-rate
conversion and fractional adjustment need their own targeted validation. B8 of
[the format research](formats-ffr-danoni.md) records v51.2.2 source-derived
semantics, while this corpus records the engine version that actually ran.
Keep that distinction when a future differential case disagrees.

### Remaining coverage checks

1. **Oracle stage and version drift:** validate adapters on actual served
   scripts, record hashes, compare captured note counts with the engine's chart
   details, and reject an analysis-only or post-mutation snapshot.
2. **Runtime custom data:** compare source inputs, effective objects and the
   three observed stages. They are not snapshots after every hook; report
   whether a static importer can represent observed changes.
3. **Incomplete multi-chart/media traversal:** visit every author chart,
   resolve per-chart audio and divided dos, then replay from cache with network
   disabled. Report chart-level failures and late media cache misses.
4. **Non-reproducibility:** record options, locale, viewport, script hashes and
   capture stages. Repeat offline and compare parsed arrays; report randomness
   or time-dependent differences without silently seeding a modified work.
5. **Historical availability and rights:** a public list entry is not proof
   that its game still runs or that its assets may be redistributed. Honour
   access rules, stop on removed/Flash-only/protected works, and keep media local.

The automated Chrome test proves two synthetic charts, inline/external merging,
custom preloading rewrites, exact strings, preblank reconversion, all three
snapshot stages, custom keys, notes/holds/speed/boost/legacy colors, encoded silent
WAV, a relative image, fetch deduplication and offline replay after the source
server closes. Unit tests cover tagged/sparse snapshots, invalid references,
schemas/paths and robots/cache/network behavior. This proves transport and
contract behavior, not support for every deployed engine/custom work.

Real coverage still needs explicit inventory evidence for split/locked numbering,
encoding flags, both color forms, deployed data-mutating JS, ordinary/encoded
audio and multiple engine generations. Include automatic preblank with modern
colors, scroll and key events before treating those semantics as covered. Assign
tags from captured state and replay results, never filenames alone.

## Native legacy pilot (2026-10-08)

Native inspection is implemented separately under
[`tools/danoni-dump/ruffle/`](../../tools/danoni-dump/ruffle/README.md). That README
specifies config fields, filenames, JSON graph representation, units, bounds,
provenance and reproduction commands. Its experimental schemas are diagnostic;
the importer must not read them as a CW `/1` reference or assume arbitrary root
properties are validated chart arrays. The agreed general export layout and
multi-chart/custom-key representation remain in the
[legacy plan](danoni-legacy-plan.md#output-choice-community-dos-plus-preservation-and-reference-data).

Original synthetic fixtures establish exact stored-array capture, repeatability,
non-invasive inspection and restricted execution. CW v48.4.1 preserves the
integer fixture's note/hold times exactly; a 240.5-frame note becomes 241 and
correctly fails comparison by 0.5/60 seconds. This is evidence for the export
validation path, not for a real Flash engine's timing/effects semantics.

The following bounded acquisition used Japanese discovery and the existing
paced, robots-aware persistent cache. Exact pages/SWF/sidecars, response hashes,
retrieval timestamps, credits and static classification are preserved under
`~/Music/DanOni/legacy-pilot/discovery/`. Prepared input directories with copied
cached blobs and `provenance.json` are under `legacy-pilot/inputs/`; the final
Higan title capture is under `legacy-pilot/captures/autumn-higan-title/`.
Nothing from these works is committed, including screenshots. These are local
research acquisitions, not complete importable song dumps.

| Local candidate | Work and credits | Observed coverage | Status |
|---|---|---|---|
| `autumn-higan` | [彼岸帰航 ～ Titanic of Stygian～](https://autumnleaves.yukishigure.com/higan/higan.html), chart レク, music どぶウサギ | Five active lanes, holds, speed, separate `iro.txt` colours; Shift_JIS ampersand dos, placeholder values; streaming sound tags in SWF. Page names source 2.15, rendered title says Ver 2.16. | Initial title failure resolved by bundled font metrics. Complete 562-tap/9-hold native projection and continuous MP3 demux now reproduce offline; reconstructed dos passes CW comparison. Original Flash latency/visual fidelity unmeasured. See [native handover](danoni-native.md). |
| `autumn-destined-marionette` | [Destined Marionette](https://autumnleaves.yukishigure.com/_9/D_M.html), chart レク, music ビートまりお | Five lanes, holds, speed including zero, boost, external dos, streaming sound tags. Exact source family/revision unresolved. | Original inputs acquired and statically classified; native initialization/export untested. |
| `cw-straight-soul` | [Straight Soul -Web Style-](https://cw7.sakura.ne.jp/danoni/2009/straights/main.html), chart ティックル, music ASK | 5-key Normal and 7-key Hard; ParaFla trace strings and `< ver fd 4.0.5 based >`; linked [HTML5 port](https://cw7.sakura.ne.jp/danoni/2009/0169_StraightSoul.html). | Original inputs acquired and statically classified; port equivalence and native export untested. Absolute ranking endpoint strings are not probed. |

All three SWFs are CWS/SWF8 with AVM1 action tags, no observed DoABC/AS3 flag,
and header FPS 60. This does not establish the chart clock or playability.
None is counted among the nine existing original browser-derived chart references.
Higan's unrelated sidebar SWF was not acquired. Public retrieval is not proof
that a work runs correctly in modern browsers or Ruffle.

Robots returned 404 at the explored origins. Reviewed Autumn pages disclosed no
additional prohibition; this does not imply a free asset licence.
[Cross Walker's terms](https://cw7.sakura.ne.jp/about.html) distinguish music
copyright and request title/maker credit for links. A separate discovery branch
at [Crepe Curtain](https://ccsyouta.sakura.ne.jp/about.html), whose terms discourage
direct media/SWF hotlinks, stopped at a catalogue JSON 403; it was not retried.
No login, challenge, ad script or ranking endpoint was used. Acquired SWFs ran
only through the separately tested native macOS restricted worker.

The narrow Higan gate now has a complete pre-consumption projection, observed
music origin and passing CW comparison. Its preserved real export is at
`~/Music/DanOni/legacy-pilot/exports/autumn-higan-normal/`; original inputs,
raw native state, parent provenance, demux metadata and browser validation remain
inside that local folder. The [native handover](danoni-native.md) specifies its
experimental schemas, distinct frame coordinates and limits. The optional mixer
advances PCM samples, while wall-clock `getTimer` still prevents declaring
general audio-synchronized emulation. Next compare a second family and the
Straight Soul archive/port; their equivalence remains unverified. Missing
cosmetics alone need not block a static chart export, but unknown note/lane/timing
behavior must. Chart Reverser is an additional source-known authoring-grid
reference, not a substitute for establishing that mapping.

Use `~/Music/DanOni/.cache` for future browser and legacy discovery. Its verified
merge imports the previously separate discovery responses and refusals without
requests. The duplicate Straight Soul HTML page and robots response keep both
retrieval histories in the merge report; no provenance was rewritten.

## 2004 Kouhaku collection milestone (2026-10-09)

The native extractor now covers a second pinned engine family: the maintainer's
[2004 Kouhaku SWF](http://eucaly.net/~iso50/kouhaku.html), five songs and six
five-key charts, including the hidden 桃源郷 Hard. All 2,493 taps agree between
selected menu data and initialized child timelines. Per-chart CW conversion and
fresh native repeat checks pass; generated five-key simfiles parse in the game.
The [collection handover](danoni-kouhaku.md) specifies the export, local inventory,
reproduction and audio limits. Its embedded source music is MP3; playable files
use untrimmed WAV. Original Flash's 1,633-sample latency/seek compensation remains
unverified, so this is not added to the zero-latency Higan category.

Import the local `~/Music/DanOni/legacy-pilot/exports/2004-kouhaku/playable/` folder
for five songs and six charts. Original works, music and raw runtime evidence
stay outside Git; only the adapter, original synthetic tests and handover are
committed. The nine original HTML5 browser references and their `/1` contract
remain a separate corpus count.

### Numeric ratings and musical tempo hypotheses (2026-10-09)

The [beat-grid handover](danoni-beat-grid.md) adds typed source star counts, an
independently validated Ogg variant and optional chart/audio tempo analysis.
Four Kouhaku tracks have strong regular-grid candidates; 北風 needs further
tempo/phase review. Estimates remain separate from authoritative note timing.
Fixing beat-based colours requires a separate musical-beat description in the
game, which remains owned by the importer/gameplay session.


### Shared pack policies (2026-10-09)

[Generic post-processing](danoni-postprocess.md) now separates normalized chart
extraction from active-preview selection, numeric-stars-to-tier heuristics,
WAV/Vorbis packaging and chart/audio grid estimates. It works on both native
families and original synthetic 7/9-key inputs; existing HTML5 dumps still need
an explicit normalization adapter. The [beat-grid handover](danoni-beat-grid.md)
records dominant-window fits and excluded sections. The latest Kouhaku ZIP is
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-generic-pack/playable.zip`; earlier
exports and raw source references remain intact. None of these policies changes
source note timing or automatically adopts a musical BPM/quarter-beat phase.


The newer `2004-kouhaku-display-bpm/playable.zip` adds explicitly reviewed
`#DISPLAYBPM` labels without changing timing/audio. The
[menu handover](danoni-postprocess.md#display-bpm-handover-exporter-and-menu-are-separate)
records the remaining game-side issue: SSC summary currently ignores that tag
and uses the synthetic 75 BPM timing. Existing stored library entries need
reimport/recomputation after the implementation session fixes the summary.


### Approximate musical-grid variant (2026-10-09)

The maintainer accepts single-digit millisecond quantization. The generic
[opt-in musical exporter](danoni-postprocess.md#opt-in-musical-timing-with-a-strict-error-budget)
now produces `2004-kouhaku-musical-grid-v3/playable.zip`, preserving all 2,493 notes
with maximum added shift 8.893 ms and full signed shift/fallback reports. It uses
fitted timing BPMs, so the existing menu shows 160/175/110/150/200 and regular
notes occupy musical colour buckets. 北風 retains 186 fine-tick fallbacks; tempo
segments and quarter-beat phase still need review. Default exact exports,
original dos/native evidence, and unchanged audio remain separate.


### Measured native-scroll trial (2026-10-09)

The newest private ZIP is
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-musical-native-scroll-v3/playable.zip`.
Six fresh restricted captures of the cached source verify a common default
120 px/s and 43 px reference arrow width. The generic
[motion/SSC policy](danoni-postprocess.md#measured-native-scroll-with-a-tunable-correction)
uses inverse-BPM scroll ratios, retaining player X-mod and the earlier musical
colour grid. Every chart's X1/X2/X3 rate and all 2,493 note timestamps are checked
through the game engine. Native motion evidence/inputs stay in the separate
`~/Music/DanOni/legacy-pilot/motion/2004-kouhaku/` archive. No work was fetched again.
The tool's `--scroll-factor` tunes the authored baseline without changing notes,
audio, BPM or user settings. Source geometry and provisional beat-phase/variable-
tempo limitations are explicit in the handover.


### First-passage previews (2026-10-09)

The latest private pack is
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-pickup-previews-v2/playable.zip`.
[The generic pickup policy](danoni-postprocess.md#earliest-music-pickup-previews)
combines sustained audio strength and nearby distinct note/hold heads, skipping
quiet or isolated openings. Twelve-second excerpts now start at
14.87/10.9033/8.5367/8.0033/11.57 seconds in song order. Higan also selects an
earlier supported passage around four seconds. Thresholds and bounded lead-in
are configurable, with explicit fallback/selection diagnostics. All accepted
musical note positions, colours, native baseline corrections and audio are
unchanged. Parser/engine checks run under the maintainer's new Rust 1.99.0;
old captures/build provenance retain their original toolchain identity.

### Artwork and title study (2026-10-09)

The [artwork study](danoni-artwork.md) records five original Kouhaku song-menu
captures, recovered link-banner JPEGs and a generic typography prototype. The
local comparison gallery is
`~/Music/DanOni/legacy-pilot/artwork/2004-kouhaku-study-v1/index.html`.
The accepted playable pack is unchanged. Source wordmarks, runtime titles and
page spellings are retained as separate evidence; no source graphic is assumed
to be a song cover simply because it is embedded in the movie.


### Index discovery and individual-song expansion (2026-10-09)

The [index-input survey and contract](danoni-indexes.md) prioritizes single-song
SWFs, with one or several difficulties, and explicitly targets 5, 7, 7i, 9A and
9B. Seven new legacy works are acquired under
`~/Music/DanOni/single-song-corpus-v1/`, with original indexes/pages/SWFs/sidecars,
HTTP and genuine-browser navigation evidence, and restricted native title
snapshots. They span three 2004 iso works, archived 2005 RE:RAVE, two 2006
festival 7-key works, and 2011 FAKE800. They are source/adapter cases, not yet
validated chart exports. Existing Higan remains the validated single-song
native case; Kouhaku remains the collection case.

Three individual-song HTML5 works add four complete selected-chart references:

| Local directory / work | Modes and captured data | Validation |
| --- | --- | --- |
| `vh-dancing-at-dusk` — [Dancing at Dusk](https://vorhandensein.sakura.ne.jp/danoni/295_dancing/) | CW v47.3.2, 5-key; 546 taps, 51 holds, 4 speed events; custom JS/title presentation | Complete work; startup/assets and reference match offline |
| `vh-canaria-7i` — [カナリア](https://vorhandensein.sakura.ne.jp/danoni/366_canaria/) | CW v47.3.2, 7i; 825 taps, 37 holds; custom JS and title PNG | Complete work; startup/assets and reference match offline |
| `vh-hikari-9ab` — [ふしぎなひかり](https://vorhandensein.sakura.ne.jp/danoni/291_hikari/) | CW v47.3.2; chart 1: 9A, 74 taps/4 holds; chart 2: 9B, 163 taps/no holds | Both selected charts complete and match offline; whole work partial because unsupported 9i chart 0 is deliberately uncaptured |

The existing local importer differential test now compares **13 chart references
with zero differences**. This includes prior unsupported-mode cases as data,
not as newly playable modes. No game/importer code was changed. Index traversal
and automatic Wayback fallback remain proposed; the survey records concrete
page/SWF/sidecar routes and mixed archive capture dates for implementation.
