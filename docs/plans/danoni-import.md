# Desktop DanOni URL imports

The opt-in `danoni` development feature connects the native SDK to Add songs:
`cargo run --features danoni`. It requires Rust **1.99** (the default game remains
on 1.98). macOS and Linux enable acquisition; Windows and the browser do not
show the URL entry. Only the SDK's reviewed Flash engines are supported, not
arbitrary DanOni pages or HTML5 works. The result uses normal SSC/Opus playback.

This feature is **not ready for redistribution**. It bundles no worker, performs
no executable downloads, and does not approve the SDK/runtime license inventory.
The root's default generated credits do not cover its optional acquisition graph;
a complete enabled-graph notice inventory, worker licenses and trusted publisher
attestation are still release gates. The standalone runtime remains separately
verified, and does not attest the final application binary.

## Explicit local installation

First build/review/install a matching runtime using
[the runtime guide](../research/danoni-rust-runtime.md). Keep its complete prefix,
managed store, acquisition cache and staging area outside every Git checkout.
The library parent must exist; the first URL import creates a missing managed
store (or initialize it with the SDK's `create-store`). Configure
`host.json` as in the runtime guide: runtime prefix, cache, staging, library,
offline preference and explicit per-origin public-access decisions. Robots
policy remains enforced; this UI never automatically approves site terms.

Build the application, then create a separate desktop binding file:

```json
{
  "host": "host.json",
  "applicationSha256": "SHA256_OF_THE_FINAL_GAME_EXECUTABLE",
  "applicationLockSha256": "SHA256_OF_ROOT_CARGO_LOCK_USED_TO_BUILD_IT"
}
```

Use `shasum -a 256` on the actual executable and the root `Cargo.lock`. Set
`DDI_DANONI_HOST` to the binding file's absolute path before starting that exact
executable. Relative `host` paths resolve beside the binding file. `host.json`
paths resolve beside that file; a work URL never chooses them. The full running
executable and embedded root lock must match, independently of the runtime's SDK
lock/source/inventory/font/worker checks. Review and recreate the binding after
any rebuild or dependency change. These operator-selected hashes provide
integrity, not publisher authentication; do not use an untrusted binding file.
Missing/incompatible configuration is explained by the import report.

In Add songs, choose **Import from DanOni URL…**, paste a complete HTTP(S) work
URL and press Import. Progress is coalesced on the background job; Cancel is a
request and publication already completed is still reported as installed.
Closing the dialog keeps the job running. Exit requests cancel and asynchronously
drain it before leaving the event loop. Startup reads the verified catalog on a
background thread; committed files stay playable without starting the worker.

## Library and recovery

Only the SDK-owned verified playback snapshot is registered, never recursive
transaction discovery. IDs remain the SDK IDs. Playback metadata adds the optional onigiri-free chart
variants from each verified SSC without rewriting admitted catalog entries. Folder/bundled songs remain in
the library; folders win a rare ID collision. Replacement retains immutable old
files for in-progress plays and invalidates future preview preparation. Removing
an imported row or pack hides it through the existing library settings; no store
files are deleted. A successful reimport restores only that work's hidden rows;
other rows of a hidden managed pack remain individually hidden. Failed/cancelled
imports and startup do not restore hidden rows. Managed packs do not offer folder re-read.

The SDK recovery journal is checked on startup/import before snapshot publication.
A failed/cancelled job reports its error and attempts to refresh the verified
published catalog, so a commit is never silently hidden by a later notification
failure. The SDK's bounded wall deadline still applies. Durable host restart/resume
controls and multi-work partial-collection review remain follow-ups; the UI does
not claim automatic resume of interrupted extraction. Staging evidence is retained
in the host-configured external area for explicit SDK diagnosis/resume.

Synthetic tests cover verified snapshot/corruption/replacement, asynchronous
folder/managed merge, URL/cancel UI and application/graph bindings. Real installed
runtime URL acquisition, audible sync, Linux interactive behavior and distribution
are separate acceptance gates; no live third-party songs are test fixtures.
