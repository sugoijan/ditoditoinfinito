# Passive source discovery and cache evidence in Rust

This is the first slice of migration milestone 5. It discovers candidate SWFs
and navigation from already acquired HTML without a browser, JavaScript runtime
or network transport. It does not yet turn an arbitrary URL into prepared songs.
The existing [automatic native profiles](danoni-rust-early-profiles.md) and
[dependency callback](danoni-rust-profile-recovery.md) remain the extraction path.

## Host API and CLI

`acquisition::inspect_page(url, bytes, encoding_hint)` accepts bounded HTML and
returns `PageDiscovery`. `inspect_cached_page(source, request)` additionally
verifies the bound source manifest and body through `AssetSource`, with one read
of each binding. `inspect_cached_page_job(path)` is the filesystem adapter; it
uses `DirectorySource`'s contained, symlink-free external roots.

```sh
cargo build --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust
target/danoni-rust/debug/ddi-danoni-export inspect-cached-page /tmp/page-job.json
```

The job is passive data, with roots relative to the job's directory or absolute:

```json
{
  "schema": "ddi-danoni-cached-page-job/1",
  "roots": {"source": "/local/corpus/work"},
  "request": {
    "manifest": {
      "namespace": "source",
      "path": "manifest.json",
      "sha256": "<SHA-256 of the existing manifest bytes>"
    },
    "url": "https://example.org/work.html",
    "encodingOverride": null
  }
}
```

Stdout is JSON; errors go to stderr with a stable code and unsuccessful exit.
The command creates no output directory, changes no cache and performs no fetch.
Redirects and request-context variants are historical observations here, not
permission to reuse a response for a different live request.

## Precise output contract

`ddi-danoni-cached-page/1` contains:

- `manifest` and `body`: exact namespace/path/SHA-256 bindings.
- `responseEvidence`: `http-response` or `archive-body-only`.
- `encodingOverride`: the explicit caller override, or null.
- `resource`: the unchanged JSON resource record, including original request,
  redirects, headers, dates, policy and browser request context when present.
- `discovery`: the `ddi-danoni-page-discovery/1` object below.

The versioned reader accepts `ddi-danoni-single-source/experimental-1` and
`ddi-danoni-resources/1`, with exactly one matching `requestedUrl`. HTTP records
must carry status 200, `completeBody: true`, `mimeType: text/html`, a retrieval
stamp, final URL, body SHA-256 and byte length. It uses `localPath`, otherwise
`bodyPath`, relative to the manifest. It verifies the bytes and recorded length.
XHTML is not parsed with the HTML tokenizer.

The older single-source archive variant has `replayUrl`, `logicalUrl`, `snapshot`
and `role: page`, with top-level `archive.retrievedOn`. Its original URL and
snapshot must agree with the explicit replay URL. The body is hash-verified;
length is checked if present. Such records lack complete HTTP response metadata.
They remain `archive-body-only`; no status, robots permission, response headers
or completeness assertion is synthesized. This reader is not a live cache-hit
policy implementation.

The discovery object contains:

- `source`, `base`: `SourceAddress` objects for the final response and effective
  document base. The source manifest/resource retain the original requested URL.
- `bodySha256`, `encoding`: exact body hash and decoder's canonical label.
- `links`: `{kind, address}` for `anchor`, `frame` and `iframe` candidates.
- `movies`: `{kind, address, flashVars}` for explicit `.swf` object/embed or anchor
  targets. Query strings remain part of the address; `flashVars` is an ordered
  string map. Identical records are deduplicated. Object and embed evidence can
  describe the same movie separately; different FlashVars must remain distinct.
- `scripts`: external script addresses as evidence only.
- `hasInlineScript`: inline script tags were present, not proof that a browser
  fallback is needed or that static discovery covered everything.
- `diagnostics`: rejected base/link/embed/script candidates. A successful parse
  can contain no movies and does not imply extraction readiness.

`SourceAddress` has `logicalUrl`, `retrievalUrl`, `capture` and `replayModifier`.
Only anonymous HTTP(S) addresses are accepted; fragments are removed and query
strings preserved. Explicit Wayback URLs accept a 14-digit capture with either
no modifier or `id_`. Wildcards, calendars, partial dates, other modifiers and
nested replay URLs reject. Relative links resolve against the logical original
URL and inherit the explicit snapshot. Already rewritten archive links preserve
their own snapshot, including mixed dates. There is no implicit live fallback
or CDX search. An inherited timestamp is a retrieval candidate, not evidence
that that capture exists; the eventual response must retain its actual capture.

## Parsing and limits

The existing html5ever tokenizer handles entities and attributes, including
noscript embed fallbacks and nested object parameter scopes. It recognizes
`movie`/`FlashVars` params and embed `src`/`flashvars`. Conflicting repeated movie
params or FlashVars keys reject that movie. FlashVars use UTF-8 form decoding,
including percent escapes and plus-as-space, and never execute as code. Non-UTF-8
encoded FlashVars need a separately evidenced adapter.

Script/style/raw text and textarea/title contents cannot introduce fake embeds
or links. The first document base applies to links before it as well as after it.
Invalid candidates become diagnostics. This is passive HTML discovery, not a
complete browser DOM or emulation of Flash embed selection. Other embed params,
including a custom Flash resource base, and JavaScript-generated markup still
need host support before a candidate can be considered a complete runtime
context. Scripts are never run by this module.

Bounds are 2 MiB per page, 4,096 bytes per URL, 2,048 raw link/movie/script
candidates, 32,768 tags and 64 nested objects. Exceeding page/structure bounds
returns `discovery-limit`, never partial discovery. FlashVars have 16 KiB encoded
length, 64 entries, 1,024-byte keys and 4,096-byte values; a bad/oversized field
rejects its candidate with a diagnostic. Manifests use the existing 16 MiB JSON
bound and at most 4,096 resource records. No recursive traversal occurs.

Decoding prefers a BOM, then the supplied HTTP/caller encoding, then the first
1,024 bytes' meta charset, then UTF-8. `cp932` maps to Shift_JIS. Invalid sequences
stop parsing instead of silently replacing characters in URLs. The caller may
supply an evidenced `encodingOverride`, retained in the output. This is an
explicit compatibility policy, not a claim to reproduce browser charset guesses.

## Validation and next integration

The source-bound local replay covers 86 cached pages: 84 HTTP records and two
older archived bodies. Default parsing finds the independently recorded work SWF
on 81 pages. Five default failures remain explicit: four pages have no charset,
and Babylon's HTTP UTF-8 declaration conflicts with its Shift_JIS meta/body.
Separate evidenced Shift_JIS overrides find the remaining five SWFs. The repeatable
gate checks 91 cases (86 original inputs plus five override trials), including
the expected original failures. It does not infer complete navigation coverage
or extraction support from an embed match.

Receipts, source-bound jobs, results and the data-only replay driver live under
`~/Music/DanOni/reviews/rust-acquisition-20261014/final/`. No requests were made.
No downloaded HTML, SWFs or music enter fixtures. Synthetic tests cover nested
object scopes, fake markup in script/raw text, entity/form decoding, ambiguity,
encodings, URL/archive identities, body/hash/path/length guards, schema versions
and resource bounds. Native SDK tests pass 173 cases; the pure configuration
remains covered separately. Re-run an existing local case list with:

```sh
DDI_DANONI_PAGE_CASES="$HOME/Music/DanOni/reviews/rust-acquisition-20261014/final/cases.json" \
  cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test acquisition local_cached_page_corpus \
  -- --nocapture
```

Next add the actual host fetch/cache policy service: historical evidence alone
cannot grant a request. Port robots/terms decisions, anonymous request contexts
and Vary matching, redirect/DNS checks, per-origin pacing, bounded decompression,
read-once persistence and cancellation. Connect it to the existing exact-GET
worker dependency callback. A selected movie's logical URL and FlashVars then
feed the source-only native context; the page alone cannot choose trusted worker
controls, profiles, filesystem paths or execution budgets.

Index traversal needs a bounded reviewed route policy with partial outcomes,
not recursive following of every discovered anchor. Known passive router formats
can follow; browser execution remains optional for genuinely script-built
navigation and HTML5 reference capture. Desktop installation and other-platform
worker confinement remain separate milestones. Accepted exports are unchanged.
