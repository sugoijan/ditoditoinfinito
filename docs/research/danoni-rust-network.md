# Rust cache replay and request-policy core

This milestone 5 slice ports historical cache replay, request-context/Vary
checks, robots decisions, DNS answer validation and pacing into the embeddable
Rust SDK. This first slice had no live HTTP client, cache writer or index crawler;
[the subsequent HTTP coordinator](danoni-rust-http.md) now supplies transport and persistence. It builds
on [passive page discovery](danoni-rust-acquisition.md); the source-only native
profiles and exact-GET dependency callback remain the extraction boundary.

## API and CLI

`network::CacheReplay::open(source, index)` takes an `AssetSource` and exact
index binding. `lookup(request)` returns one of:

- `Hit(Box<ReplayResponse>)`: one actual HTTP response, with shared immutable body
  bytes and source bindings. A redirect remains a redirect.
- `Miss { requested_url, canonical_url }`: no corresponding entry. This is not
  permission to fetch it.
- `RecordedFailure { requested_url, canonical_url, index, record }`: the exact
  recorded refusal/error. It never becomes a miss or automatic retry.

Errors are separate: malformed/corrupt cache data, incomplete bodies, unsupported
or mismatched Vary, request context and size limits do not become misses.
`ReplayResponse::receipt()` omits body bytes and retains request/context, canonical
and response URLs, status, headers, redirect target, index/body bindings, byte
length, unchanged resource record and the selected response/hop record.

```sh
cargo build --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust
target/danoni-rust/debug/ddi-danoni-export replay-cache /tmp/cache-job.json
```

The job's roots are absolute or relative to its directory:

```json
{
  "schema": "ddi-danoni-cache-replay-job/1",
  "roots": {"cache": "/local/corpus/.cache-v2"},
  "index": {
    "namespace": "cache",
    "path": "index.json",
    "sha256": "<SHA-256 of the existing index bytes>"
  },
  "request": {
    "url": "https://example.org/work.html",
    "context": {"kind": "anonymous", "headers": {"accept": "*/*"}},
    "maximumBytes": 2097152
  }
}
```

`context` defaults to anonymous and `maximumBytes` to 128 MiB. Stdout is
`ddi-danoni-cache-replay/1` with `result` (the receipt) and `networkRequests: 0`.
A miss or recorded failure is a successful inspection command with its distinct
result kind; neither denotes a playable work. API errors go to stderr and exit
unsuccessfully. No body is copied, request performed or cache changed.

## Historical cache contract

The reader accepts `ddi-danoni-cache/1` only, preserving the original records.
This complements the earlier resource/single-source manifest reader, rather than
replacing it. `archive-body-only` records are not fabricated HTTP cache entries.
The filesystem adapter uses `DirectorySource` containment and symlink checks.

The index is hash-verified and read once. URL keys must already be canonical
anonymous HTTP(S) URLs and agree with record `requestedUrl`. An entry cannot also
be a recorded failure. Explicit same-origin aliases may resolve through at most
64 links; duplicate definitions, cycles and cross-origin aliases reject. Queries
are preserved unless that exact alias is recorded. No new timestamp alias,
query stripping or old-cache migration is guessed.

Lookup checks the selected response's context fingerprint if recorded. Body
paths must be exactly `blobs/sha256/<digest>`, relative to the bound index, with
lowercase SHA-256, recorded byte length and a bounded retrieval stamp. Partial
206/incomplete responses reject before body use. Bodies are read once per binding
and shared across equivalent requests/aliases; every use checks its hash and
bound. Each session retains an immutable index snapshot, so a newly written
index needs a new bound view.

Limits are the existing 16 MiB index JSON bound, 16,384 entries and failures each,
4,096 aliases, eight recorded redirect hops, 128 headers, 128-byte header names,
16 KiB header values, 128 MiB per response and the existing 512 MiB `Inputs`
session limit. Header names must be lowercase HTTP tokens and values contain no
control characters. Request-specific bounds should be tighter: use the worker
callback's `maximum_bytes` for a dependency and the page/robots limits for those
roles. A larger collection needs separate bounded work sessions.

For a collapsed old redirect record, lookup selects the first recorded hop and
its original body/status/Location. The next lookup applies the target's own Vary
rules. It does not return the final body while skipping intermediate response
checks. Reviewed old timestamp migrations can retain a different same-origin
final retrieval URL only with exact recorded `cacheMigration.sourceUrl` or
`aliasOf` evidence. Original dates, redirects, robots records and contexts remain
available in the unchanged resource JSON.

## Request contexts and Vary

`RequestContext` admits only actual `accept`, `origin` and `referer` headers.
Anonymous is exactly `Accept: */*`; browser context is explicitly supplied by
the host, not extracted as executable instructions from a work. Header values
are bounded ASCII without controls, Origin is a serialized HTTP(S) origin or
`null`, and Referer is anonymous HTTP(S). Cookies, Authorization, Range, custom
headers and mutations are outside this request model.

The fingerprint matches the old fetcher's SHA-256 of compact JSON sorted header
entries. Missing old context means the original anonymous profile; a declared
fingerprint must agree. With no Vary, another admitted context can reuse bytes
while the original captured context remains unchanged. Declared Accept, Origin
and Referer compare their exact effective values. Fixed User-Agent, Accept-Encoding,
Host and absent Cookie/Authorization/Range are also understood. `Vary: *`, unknown
fields or mismatched values refuse reuse without refetching.

The negotiation profile is `DdiDanoniDump` and `gzip, deflate, br`, matching the
old cache. A future transport must implement that exact profile, or explicitly
version it and refuse incompatible cached variants. It must not silently use a
client's different defaults or upgrade this reader into authenticated requests.

## Robots, DNS and pacing

`CacheReplay::robots(target)` follows only existing bound `/robots.txt` responses,
including at most eight redirects. Each Location must match the next exact
request, every body/status is checked, and the terminal response must be 200 or
404. Loops, missing targets, cache/variant errors and other statuses stop. It
returns `RobotsDecision` with the original target origin's policy URL, final
retrieval URL, full response receipts, final body/index bindings, retrieval stamp,
User-Agent, allowed flag and optional millisecond crawl delay.

`evaluate_robots` and `evaluate_robots_chain` expose the same pure evaluation for
bound responses. The [robotstxt 0.3.0 dependency](https://docs.rs/robotstxt/0.3.0/)
is the Apache-2.0 Rust port of Google's matcher, verified as the latest release
for this slice and already available in the local Cargo cache. No upstream code
is copied into the repo. Bodies are bounded to 512 KiB. Status 404 means no rules;
200 is strict UTF-8 (optional BOM), with no NUL or replacement decoding.

The small Crawl-delay adapter uses the parser's callbacks and matching agent
groups. Specific-agent groups supersede global groups, including a specific
group without a delay. Repeated matching delays use their maximum; invalid,
negative, non-finite or greater-than-one-hour selected delays stop rather than
being clamped down or ignored. Fractions round up to milliseconds. This is
conservatively stricter than the old extension's last-value/invalid-value behavior,
without changing ordinary allow/disallow matching or any corpus policy outcome.

`pin_public_addresses(url, ips)` accepts 1..64 resolver answers, checks every
answer, rejects mixed public/private sets and returns deduplicated SocketAddrs
with the URL's exact HTTP port. Literal-IP URLs must agree with all answers.
Private, loopback, link-local, multicast, documentation and the supported
reserved IPv4 ranges reject; IPv6 admits an ordinary global-unicast subset and
also rejects mapped addresses, Teredo, 6to4, benchmark/ORCHID and documentation
ranges. This ports the old conservative scope with additional explicit IPv6
exclusions; it is not a claim to accept every publicly routable special address.
The [IANA IPv4](https://www.iana.org/assignments/iana-ipv4-special-registry/iana-ipv4-special-registry.xhtml)
and [IPv6 registries](https://www.iana.org/assignments/iana-ipv6-special-registry/iana-ipv6-special-registry.xhtml)
are the reference for those exclusions. The eventual client must dial only the
checked answers, preserve TLS hostname verification, and disable proxy, implicit
redirect, retry and second-resolution fallbacks. This helper does not itself
perform DNS, connect to a host or enforce a DNS deadline.

`Pacer` uses a host-supplied monotonic millisecond clock, with a minimum one-second
interval per origin (including scheme/port) and the larger selected crawl delay.
`ready_at` only computes a time; `mark_started` consumes a slot and rejects early
starts, clock rollback/overflow, overlarge delays or more than 256 origins.
An unstarted cancellation consumes no slot. The host must own synchronization,
perform cancellable waits and call `mark_started` at the actual request boundary.
There is no blocking sleep in this policy core.

These are historical decisions and pure checks, not current request authority.
The live coordinator still needs robots freshness, site terms, total deadlines,
DNS cancellation, encoded and decoded body bounds, exact HTTP execution,
read-once persistence, single-writer locking and observed dependency integration.

## Validation and next work

The independent offline oracle uses the existing Node `CaptureNetwork` and
robots parser. It disables the live request method and redundant output copying;
original cache indexes remain byte-identical. Inputs, oracle drivers, expected
results and receipts stay under
`~/Music/DanOni/reviews/rust-network-20261014/`, never in fixtures.

The gate covers both `.cache` and `.cache-v2`: 1,629 requests, including 373
explicit alias requests. All 1,615 responses, 12 recorded failures, two unsupported
Vary cases and seven redirect responses match. All 1,624 historical robots
decisions (1,623 allow and one deny) and five unavailable policies match too.
That includes the FC2 robots HTTP-to-HTTPS redirect exposed by the corpus. The
combined gate reads all 941 bound files exactly once and performs no request.
All 188 native and 85 pure SDK tests, both strict Clippy configurations, format
checks and `just check` pass (821/821 REUSE files). Five CLI trials cover hit,
redirect, refusal, incompatible variant and miss. Validation receipts and log
hashes stay with the local oracle. Original synthetic tests cover non-reuse,
corruption, bounds, arbitrary query
changes, alias cycles, redirect target Vary, missing/wrong-origin/looping robots
chains, group specificity, delay handling, mixed DNS answers and cancelled waits.

```sh
DDI_DANONI_CACHE_CASES="$HOME/Music/DanOni/reviews/rust-network-20261014/cases.json" \
  cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test network_corpus -- --nocapture
```

No accepted export is regenerated or promoted. The subsequent
[HTTP coordinator](danoni-rust-http.md) implements anonymous transport and bounded
persistent cache publication using this policy core. Joining the exact-GET
dependency callback and adding reviewed index routes remain next work. The initial
shell DNS failure was subsequently resolved with operation-scoped
`100.100.100.100`; the HTTP slice uses the latest released client.
The robots dependency was resolved offline and `cargo update --offline` changed
only its addition. Browser/HTML5 control, desktop installation and other-platform
worker confinement remain separate milestones.
