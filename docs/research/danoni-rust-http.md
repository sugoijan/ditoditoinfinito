# Rust HTTP acquisition and persistent cache

Implemented 2026-10-14 within [migration milestone 5](danoni-rust-plan.md).
This joins the [request-policy core](danoni-rust-network.md) to native async HTTP
and durable cache writes. It returns verified response evidence; it does not yet
turn a work URL into installed songs. No third-party work is a repository fixture.

## Embedding contract

The `native` feature exports `network::{Fetcher, FetchOptions, TransportOptions,
NetworkCancel, AcquiredResource, TransferCounts}`. Open one `Fetcher` with an
external cache directory, then await `fetch(&ReplayRequest, &NetworkCancel,
&mut site_policy)`. A Tokio runtime with I/O and time enabled is required.
The host callback receives each new URL and returns a bounded, nonempty reason
for permitting anonymous public access, or an error. It must review site terms
and access restrictions; neither page scripts nor robots.txt grant that decision.
Use `site-terms-denied` for a definite refusal, which is persisted without fetching
the target. Cache hits require no new site-policy callback or network request.

`AcquiredResource` contains the canonical requested URL and a nonempty ordered
`responses` array of `ReplayResponse` values, including each actual redirect hop.
`response()` selects its terminal response. HTTP errors are response evidence,
not successful work imports. Each response exposes shared decoded bytes, status,
headers, immutable index/body bindings and original metadata. The namespace is
`cache`, rooted at `Fetcher::cache_root()`. Existing source discovery and dependency
validation can consume those bindings. A dependency resolver must still require
HTTP 200 and bind the logical request URL separately from the retrieval URL;
Wayback originals must not be replaced by replay host URLs.

Clone a `NetworkCancel` and call `cancel()` to interrupt DNS, pacing, connection,
body reception or decoding. Cancellation is sticky. File publication is synchronous
and durable: a complete response may finish publishing before cancellation is
reported, but does not become prepared/playable songs as a side effect.

Current defaults and hard bounds:

| Option (JSON name) | Default | Admitted range |
| --- | ---: | --- |
| `transport.tlsRootBundle` | platform trust | optional absolute PEM path + SHA-256 |
| `transport.dnsServer` | system resolver configuration | optional IP address |
| `transport.requestTimeoutMs` | 30000 | 1–60000 ms |
| `maximumRequests` | 64 | 1–64 new GET attempts |
| `maximumWireBytes` | 268435456 | 1–536870912 bytes |
| `maximumDecodedBytes` | 268435456 | 1–536870912 bytes |
| `wallMs` | 180000 | 1–300000 ms per Fetcher lifetime |
| `robotsMaxAgeSeconds` | 86400 | 1–86400 seconds |
| `offline` | false | boolean |

A request's `maximumBytes` also bounds its decoded body. Robots bodies are at most
512 KiB; response headers are at most 128 fields, 64 KiB combined and 16 KiB per
value. The body pool holds at most 512 MiB per Fetcher session; release the session
between bounded jobs. Request counts include robots and redirect GETs. Byte counts
include received/decoded chunks on failed transfers and may exceed the limit by
the final rejected chunk. Cache hits transfer zero bytes and consume no GET budget.
The total deadline includes policy acquisition and pacing; each live hop also has
a request deadline. Start a new bounded job rather than extending an expired one.

## Transport and policy

Only anonymous GETs are acquired live: `Accept: */*`, `User-Agent: DdiDanoniDump`,
`Accept-Encoding: gzip, deflate, br`. Historical browser-context records can still
be replayed, but live browser-context acquisition is refused before any GET.
There are no cookies, credentials, Range requests, proxy inheritance, automatic
redirects or HTTP retries. TLS verifies the original hostname using rustls and
platform trust by default. A host may explicitly select `tlsRootBundle` with an
absolute regular-file `path` and exact lowercase `sha256`. The bounded (1 MiB,
1–1024 certificates) PEM bundle then supplies the trust anchors; certificate chain
and hostname verification remain enabled. Its digest is recorded as `network.tlsTrust`.
There is no automatic fallback after platform-verifier failure. This permits a
host-managed CA bundle where platform trust IPC is unavailable, without accepting
invalid certificates. System bundle updates require an explicit hash update.
TLS key logging is disabled. HTTP/1.1 keeps peer verification
explicit. Ordinary HTTP remains available for public legacy sites.

Hickory resolves the complete A/AAAA answer set asynchronously; every address must
pass the existing public-address policy. The client pins those exact addresses,
forbids a second resolution and checks the connected peer. Mixed public/private
answers fail. Explicit `dnsServer: "100.100.100.100"` uses that resolver only for
this operation and does not change system settings. Failure to read system DNS
configuration is an error, never an implicit switch to a different resolver.

Each redirect is explicit, limited to eight hops and checked for loops. A new
origin needs its own host site-policy decision and robots chain. Robots redirects
also retain every response and need a terminal 200 or 404; other outcomes stop.
Every cached robots hop must be no older than `robotsMaxAgeSeconds`, with at most
five minutes of future-clock tolerance. Stale policies are not silently reused or
refetched: use an explicitly fresh acquisition cache generation. Versioned policy
refresh without redownloading work bodies is future work.

New requests wait at least one second per origin, or longer for Crawl-delay.
Monotonic pacing handles the active session; persisted completion/failure dates
conservatively maintain pacing across restarts. Unknown clock state fails closed.
Retry-After handling and global scheduling across separate cache roots are not
implemented. Use a shared cache/scheduler for concurrent imports to one origin.

Identity, gzip (including concatenated members), zlib-wrapped deflate and Brotli
are decoded with separate wire/output bounds and deadline/cancellation checks.
Truncated streams, trailing encoded bytes, unsupported/stacked encodings and
unrequested partial responses fail. The exact decoded body is hashed and cached;
content-encoding/wire length are kept in network provenance rather than confused
with decoded length.

## Disk and receipt schemas

The mutable index remains `ddi-danoni-cache/1`, preserving old entries, aliases,
failures and unknown fields. New records are direct HTTP responses with empty
`redirects`; the acquisition receipt represents their ordered chain. New records
carry `role`, actual `retrievedAt`, request context/fingerprint, `sitePolicy`,
`network` (resolver, validated socket addresses, protocol, encoding and wire bytes),
and bound robots decision evidence. Existing replay validation remains mandatory.

```text
<external-cache>/
  .writer.lock
  index.json
  blobs/sha256/<decoded-body-sha256>
  r-<sha256>.json
```

`r-*.json` is an immutable minimal cache index containing the selected response or
failure and exact alias chain. Bindings point to this file, not mutable index.json,
so later writes cannot invalidate old provenance. Body paths stay relative to the
same cache root. Never relocate the capsule without its referenced blobs.

One advisory writer lock is held for the Fetcher lifetime. Existing files must be
regular files without symlinks. Immutable blobs/capsules use create-new semantics
and verify existing content. Index updates compare the previous digest, write and
sync a temporary file, rename it and sync the directory. A failed index publication
poisons the writer. Old Node writers do not honor this new advisory lock: do not
run them concurrently on the same cache. Digest checking detects changes but is
not an atomic compare-and-swap against an uncooperative writer.

Before dispatch, the index durably records a failure-shaped reservation with
`code: "request-incomplete"`, `attemptId` and `failedAt`. A known cancellation before
dispatch removes the reservation. After dispatch, failure/cancellation persists;
a restart annotates incomplete reservations with `recoveredAt` and refuses retries.
Success writes the immutable body before atomically replacing the reservation with
the response. Ambiguous crashes can conservatively refuse an unfetched URL; they
cannot silently fetch the same URL twice. Explicit operator recovery is future work.
Corruption, a recorded refusal, incompatible Vary or a missing reviewed alias target
also fails without refetching. A newly received `Vary: *` response is usable once;
subsequent reuse is refused.

Durable cache writing is currently supported on Unix only. Other platforms return
`unsupported-cache-platform` until their directory-sync/publication semantics are
implemented and tested. The pure export core does not acquire these dependencies. Native dependencies added
for this slice are reqwest 0.13.5, Tokio 1.53.3, Hickory 0.26.3,
async-compression 0.4.50 and time 0.3.55, checked against the official Cargo
sparse index before addition; the standalone lockfile was refreshed with
`cargo update`. No dependency or lockfile in the main workspace changed.

## CLI

`fetch-url JOB.json` accepts this exact top-level schema (unknown fields fail):

```json
{
  "schema": "ddi-danoni-http-job/1",
  "cache": "/absolute/external/cache",
  "options": { "transport": { "dnsServer": "100.100.100.100" } },
  "request": {
    "url": "https://reviewed.example/work.html",
    "context": { "kind": "anonymous", "headers": { "accept": "*/*" } },
    "maximumBytes": 16777216
  },
  "origins": { "https://reviewed.example": "Host-reviewed public site and terms" }
}
```

Relative cache paths resolve beside the job file; its parent must exist and the
cache must be outside any Git checkout. The `origins` map is a caller-authored
access decision, not a bypass for robots. Include every reviewed redirect origin.
Do not copy this example reason without reviewing the actual site.

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- fetch-url /outside/repo/job.json
```

Successful stdout is `ddi-danoni-http-acquisition/1` with `cache`, `resource` and
`transfers`. `resource` is `ddi-danoni-http-resource/1`: `requestedUrl` plus ordered
`responses`, each the existing `ReplayResponse::receipt()` shape documented in
[the cache contract](danoni-rust-network.md). Failures use the existing CLI error
protocol and nonzero exit. Set `options.offline: true` to prove replay without DNS,
policy refresh or HTTP. The embeddable async API is the cancellation interface;
the synchronous CLI does not yet expose a signal-to-cancellation bridge.

## Validation and remaining integration

All 202 native and 85 pure SDK tests pass, including the 1,629-request/robots
historical cache oracle and 91 source-discovery cases from 86 local pages.
Both feature configurations pass strict Clippy; formatting and `UV_OFFLINE=1 just
check` pass (828/828 REUSE files). The 14 new network tests are original fixtures.
The accepted export index, registry and both original cache indexes retain their
previous hashes. Validation logs and their digests are kept locally with the
HTTPS/offline receipts.

Original localhost HTTP/DNS fixtures exercise exact headers, redirect policy,
restart/offline replay, stable bindings, lock conflicts, pacing, interrupted and
incomplete attempts, robots denials/staleness, Vary, corrupt bodies, symlinks,
uncooperative writers, codec bounds/truncation/trailing data, partial responses,
trickling deadlines, empty-member decoding cancellation, optional historical
index collections, DNS cancellation and mixed-address refusal. Loopback access
is enabled only for exact fixture origins under `cfg(test)`; production has no
option to enable it. No pack code executes in these tests. The live platform-trust probe in this
agent environment stopped before an HTTP response with macOS `OSStatus -26276`;
the failure remains recorded and is not retried. The optional explicit bundle
uses normal in-process verification; no system trust settings are changed.
With the bound `/etc/ssl/cert.pem` bundle and requested resolver, a live HTTPS
probe acquired example.com robots (404) and its page (200): two GETs, 668 encoded
bytes and 1,154 decoded bytes. Offline replay returned an identical resource
receipt with zero requests/bytes; both immutable bindings were rehashed. Two
additional CLI trials replayed anonymous and browser-context historical records
from copied local cache evidence without network access. These
local receipts stay under `~/Music/DanOni/reviews/rust-http-20261014/`.
For the underlying trust choices see the
[rustls verifier deployment guidance](https://github.com/rustls/rustls-platform-verifier#deployment-considerations)
and [reqwest's explicit certificate API](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.tls_certs_only).

Next connect this async fetcher to the native dependency loop and static source
discovery, then prove bounded real work URL → prepared songs against frozen corpus
oracles. Add reviewed index routes and archive logical/retrieval mapping after
single-work paths. HTML5 browser control, desktop library installation and
non-macOS worker confinement remain separate work. Accepted exports are unchanged.
