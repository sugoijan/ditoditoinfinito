# Native DanOni importer helper

`ddi-danoni-importer` is an owned application child, built with Rust 1.99 and the
native/Opus SDK. Both are root workspace members sharing the root lockfile;
the pinned Ruffle worker keeps its independent generated workspace. It performs
URL acquisition and managed-store admission through Rust APIs, not by invoking the exporter CLI.
The main application remains on Rust 1.98 and uses only `ddi-importer-protocol`.

The packaged layout is `bundle/bin/ddi-danoni-importer` beside
`bundle/runtime/`, the complete existing SDK installed-runtime shape. The helper
resolves this relative to its own executable, never from an acquired URL, an
application identity binding JSON, or an environment-selected research runtime.
`Installation::load` still checks the SDK/runtime identities, full inventory,
fonts, notices and independently confined worker. The outer application bundle
also binds the helper's actual lock graph and notices; the shared root lock
alone does not attest the helper executable or approve redistribution.

Run with `--stdio` (or no arguments) using anonymous stdin/stdout pipes.
**Stdout contains only framed protocol events.** Human-readable startup errors
and panic diagnostics go to stderr. `--check-bundle` validates the relative SDK
runtime and exits without network requests, user storage or imported works;
outer manifest/inventory verification belongs to the application/packager.

## Build and distribute

Use the app's `--features danoni-importer` to prepare this helper and its pinned
worker/runtime automatically. This uses build-time Python 3.11+, Java, Git and
native Rust/C++ tools; none is needed by the finished package. The build driver
uses separate external caches and Cargo target directories, validates the exact
SDK subgraph shared by both locks, and refuses incomplete notices. Do not copy
a development helper executable alone.

After generating the song/noteskin indexes and building the app, run:

```
python3 tools/danoni-importer/build.py package --app target/debug/ddi --output /tmp/ddi-package
```

Move the whole output directory, then run `./ddi
--danoni-importer-check` there to exercise the actual client/helper handshake.
See [the distribution guide](../../docs/plans/danoni-importer.md) for platform
prerequisites, video compliance payload, exact notices and build-only cache
inputs. `DDI_IMPORTER_TEST_ONLY=1` skips preparation only for debug IPC checks;
those unbound binaries cannot pass packaging.

## Protocol and ownership

The shared crate defines protocol version 1 and a 32 MiB limit per frame:
4-byte unsigned big-endian JSON byte length followed by exactly those UTF-8 JSON
bytes, without a trailing newline. Header/payload truncation differs from clean
EOF between frames. Unknown fields and unknown message types fail closed.
There are no config/progress/result JSON files used for application IPC. The
SDK's durable evidence, recovery journals and managed catalog remain internal
storage, and acquired third-party data never enters the bundle or repository.

The client sends Hello once, then Initialize once with private absolute storage
paths and previously reviewed exact-origin decisions. Initialize verifies the
bundled runtime and current store, streaming SnapshotBegin/SnapshotSong/
SnapshotEnd before Initialized. Source catalog metadata is original-only: the
application may derive playback variants from verified SSCs after receiving a
complete snapshot; the helper never rewrites the admitted catalog for UI choices.

Import ids are nonzero and strictly increasing per helper session. Only one SDK
job runs at a time. Progress stages and wakeups are coalesced. New site origins
produce OriginRequired; an explicit OriginDecision with a nonempty note allows
that origin, None denies it. Approval waits are cancellable and expire after
five minutes. Robots policy is still enforced. The offline flag uses SDK cached
acquisition and does not relax its validation/site policies.

After an import attempt the helper streams the verified current snapshot before
Terminal. A post-commit refresh failure retains the truthful installed outcome,
clears restoreIds and includes a refresh diagnosis; the client must retain its
old library unless a matching complete End arrived. Successful restoreIds apply
only to completed verified snapshots. Cancellation before publication is normal;
a late cancel cannot turn a completed commit into a cancelled import.

Stdin decoding remains live on its own bounded command queue while SDK work or
origin approval runs on an owned job thread. Cancel requests never join live work.
EOF and Shutdown cancel and drain owned work before Bye. The client must continue
draining stdout during shutdown (or close the pipe if it is disappearing), and
must discard incomplete snapshot streams. Fatal framing/protocol errors also
cancel/drain rather than publishing partial library data.

Ordinary SDK cancellation tears down its confined worker process group. Linux's
worker has a parent-death SIGKILL policy. macOS hard process crashes still rely
on the SDK worker's existing confinement/execution/CPU/file limits; graceful
shutdown and forced application-helper teardown must not be conflated. No
unconfined fallback is introduced.

## Synthetic checks

Use `cargo clippy --locked -p ddi-danoni-importer
--all-targets -- -D warnings` and `cargo test --locked -p ddi-danoni-importer`, plus root `cargo test -p ddi-importer-protocol`.
Tests include real executable anonymous pipes, handshake/EOF/truncation, explicit
origin allow/deny/cancel, snapshot framing and unchanged original metadata. They
use no live URLs or installed third-party songs. Agent runs use a scratch
`DDI_HOME`, the repository watchdog and two compiler jobs.
