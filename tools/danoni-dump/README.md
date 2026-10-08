# DanOni browser dump

Capture a publicly playable Dancing☆Onigiri CW Edition work into a local research
corpus. The tool uses the installed Chrome through `playwright-core` and observes
the work's own engine. Downloaded JavaScript runs only inside that browser; Node
parses reviewed engine source as syntax to locate debugger capture points.

The survey and importer-facing contract are in
[danoni-corpus.md](../../docs/research/danoni-corpus.md). Machine-readable schemas
are in [schemas/](schemas/).

## Requirements

- Node.js 22 or later.
- Google Chrome installed locally. No bundled browser download is necessary.
- `npm ci` in this directory to install the pinned tool dependencies.

Keep the output and persistent download cache outside the repository. Works,
music, artwork, and third-party custom scripts belong in the local corpus, never
in committed fixtures. The tool and its entirely synthetic test fixture are MIT,
covered by the repository's `REUSE.toml`.

## Run

From this directory, capture all charts discovered in a work:

```sh
npm ci
node cli.mjs capture 'https://example.org/work/index.html' \
  --out "$HOME/Music/DanOni/example-work" \
  --cache "$HOME/Music/DanOni/.cache" \
  --terms-notes 'Public work; reviewed the linked site terms on YYYY-MM-DD.'
node cli.mjs verify "$HOME/Music/DanOni/example-work" --offline
```

Replace the example URL with the actual work page, preserving meaningful query
parameters. The output must be a new or empty directory outside any Git checkout;
nonempty directories are not overwritten. The optional cache defaults to the output
directory. A shared external cache avoids downloading the same resources again.

`--charts 0,2` selects zero-based chart indices. Individual selected charts can be
complete while the work manifest remains partial because other author charts were
not captured. `--timeout-ms 120000` sets the browser operation timeout in
milliseconds; 120000 is the default. `--terms-notes` records the operator's review
without automatically approving a site's terms.

For a source hash you have inspected for compatibility with this adapter, add
`--trust-engine SHA256`; the option may be repeated. Use the hash of the
browser-decoded source, not an assumed version label or a hash of compressed
response bytes. Normal captures use the reviewed hashes in `engine-hashes.json`.

Both commands write progress to stderr and a JSON result to stdout. Exit status
0 means the command completed without a failed selected chart or verification;
2 reports an incomplete selected chart or failed replay; 1 reports an invocation
or fatal error. Inspect the manifest and diagnostics even on status 0, especially
for a deliberately selected chart subset. Verification requires `--offline` and
checks captured asset hashes and fresh Chrome converter results without fetching
missing resources from the network.
`verification.json` also reports `comparisons[].referenceMatched`: a reference
can match while the overall replay fails because an asset is missing. Keep those
results distinct when using a partial dump for importer comparisons.

## What the capture means

There are three different forms of chart data:

1. Resource bytes and the exact strings supplied to the engine's `dosConvert`.
   A work can supply several strings through inline HTML and generated external
   JavaScript. These strings retain their order and decoding flags.
2. Effective engine input after merging and custom-script rewrites. This is an
   object snapshot; serializing it as pipe-delimited text would not recreate an
   original dos string.
3. The engine's full `scoreConvert` return, normalized into a reference parse
   without reimplementing the chart parser. Every captured invocation is kept,
   including a second invocation when the engine adds preblank frames.

Frames in the reference are the engine's converted frames at the captured settings.
They may include adjustment, playback-rate conversion, and preblank padding. Use
the recorded timing inputs when comparing an importer, and do not assume they are
identical to the source integers. Holds retain their flat endpoint arrays as well
as complete endpoint pairs. Resolved tap and hold field names come from the
engine, including legacy names such as `sfrzLeft_data` and `frzLdia_data`.

Engine snapshots preserve sparse arrays, undefined values, special numbers, and
object references through a tagged JSON encoding. Functions and accessors are
diagnosed as opaque; snapshot decoding never reconstructs executable code.

## Fetching and replay

The network layer uses anonymous requests, checks robots.txt, spaces requests to
each origin, and stores response bodies in a persistent cache. An offline replay
serves captured responses under their original URLs, preserving relative paths
and browser-origin behavior. Keep each cache to one writer at a time.

Use only public works whose site terms permit this local research. Do not provide
credentials, use authenticated browser profiles, or bypass removed content,
robots exclusions, or other restrictions. The tool does not grant redistribution
rights in a captured work. Review the work's attribution and terms before fetching.
The tool records terms notes and visible terms-like links; it does not
automatically review linked terms. Browser requests retain their actual Accept,
Origin and Referer, while cookies and Authorization are never forwarded. Cache
reuse respects `Vary` and fails on a mismatched request context.

## Limits and diagnostics

Only explicitly reviewed engine hashes can produce a trusted reference parse.
An unfamiliar or minified engine is not automatically trusted because its version
label resembles a supported release. The observed script URL and hash distinguish
the deployed engine from mutable version manifests and historical page dates.

Custom JavaScript can change data after parsing, implement course transitions,
or alter gameplay on every frame. A converter snapshot captures a specific stage;
it does not claim to preserve arbitrary executable behavior. Native snapshots and
diagnostics accompany the normalized data so a missing field or unsupported
extension stays visible.

The normalized reference selects the last full converter return for the requested
chart. Separate `prepared` snapshots precede `pushArrows`; `runtime` means entry
to `mainInit`, before its first statement, **not after its custom hooks**. One
active key pattern is captured per chart; the CLI does not enumerate or select
alternate patterns. Read the recorded pattern and effective settings.
The known music-selection extension is driven through its slider and Enter
handler; arbitrary custom menu implementations are not generally supported.

Embedded frames, later course/scene transitions and arbitrary future JS requests
are outside capture scope. Current images, preloads, parsed `htmlText` assets and
accessible CSS URLs are collected; unrequested assets inside cross-origin CSS
inaccessible through CSSOM are not exhaustively discovered. Offline verification
compares startup converter results and observed assets, not full-song behavior.

Encoded-music scripts, ordinary media, gameplay images, and previews are different
resource roles. In particular, a streaming preview does not establish which music
the selected chart plays. Completion and offline verification should be assessed
for the selected chart and assets, rather than inferred from the title screen.

## Tests

Run `npm test` from this directory. The synthetic fixture in
[test/fixture.mjs](test/fixture.mjs) generates its own HTML, JavaScript, SVG square,
and silent PCM WAV. It exercises multiple charts, custom key metadata, inline and
external input merging, runtime input rewrites, frame conversion, preblank
reconversion, relative assets, and encoded music. It is a transport and capture
fixture, **not** a substitute for comparison with a real danoniplus engine.
