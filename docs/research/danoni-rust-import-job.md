# Native import job coordinator

For the relocatable tooling installation, packaged Japanese fonts, reusable host
defaults and current Linux proof, see [the installed runtime](danoni-rust-runtime.md).
The dated results below preserve their original scope.

`import_job` connects URL preparation, validated admission, duplicate planning and
library installation through one blocking SDK call. It is native-only; the web
workspace and main-owned app/library code remain unchanged. Host services are
configured once: cache, trusted confined worker, optional font assets, site-access
policy, initialized target store and a background job queue. Each ordinary work
needs only its public page/SWF URL. Source support is still the proven native
engine/profile subset, not arbitrary index pages or HTML5 engines.

## SDK entry points

```rust,ignore
let request = import_job::Request::new(url);
let services = import_job::Services { cache, worker, assets };
let result = import_job::execute(
    &request, &services, new_candidate_directory, &mut store,
    &cancel, &mut site_policy, &mut progress,
)?;
// Inspect result.installation: installed / unchanged / needs-recovery.
```

The host starts its own thread or blocking task and owns the cancel token and
progress channel. No hidden thread, async executor or UI is created. Calling from
an entered Tokio runtime returns `invalid-import-job`; use its blocking pool.
`CatalogStore` extends `import_host::LibraryStore` with `snapshot()`; the managed
filesystem `Store` implements it. A database host can implement the same trait.
The existing atomic revision and operation-precondition contract still applies.

The convenience call borrows the target store for its duration. Its filesystem
handle keeps its existing exclusive writer lock; queue imports and supply cloned
catalog metadata to the UI. Hosts that need to defer obtaining their library
handle can call `url_prepare::execute` separately, bind the resulting candidate
and call `resume` once they have the handle. Library playback/read scheduling is
still the desktop shell's responsibility.

`Request` JSON is camelCase, rejects unknown fields, and contains:

```text
{
  preparation: url_prepare::Request,
  duplicatePolicy: "reject" | "keep-existing" | "replace", // default reject
  wallMs: integer                                         // default 600000
}
```

Preparation retains its existing default 300,000 ms / 2 GiB limits and policies.
The coordinator's wall limit is 1–900,000 ms, starts at entry and covers the whole
job up to commit. It is checked before/after progress callbacks and at admission,
planning and staging checkpoints. It is cooperative, not a CPU deadline,
filesystem quota or interruption of a native store write. After the final ready-to-commit checkpoint,
the store's non-cancellable finalization/publication and recovery protocol
determines the outcome.
Fresh musical fits remain provisional; source times, scroll, previews, ratings,
artwork and exact admission checks are unchanged.

## Candidate checkpoint and restart

When preparation succeeds, the coordinator binds the **outer** Ready manifest
and writes `import-candidate.json` into the fresh candidate directory before
accessing the library snapshot or starting installation:

```text
{
  schema: "ddi-danoni-import-candidate/1",
  directory: absolute UTF-8 external candidate directory,
  readySha256: lowercase SHA-256 of that directory's manifest.json
}
```

`Candidate::bind(directory)` reads/hashes that manifest. It is a locator, not an
admission proof or an assertion of success. `Candidate::read(file)` loads a bounded
16 MiB JSON token; unknown fields, relative/non-UTF-8 directories and invalid hashes are
refused. `resume` verifies the exact expected Ready bytes through `AssetSource`
before parsing them, then revalidates all bound source/runtime/preparation proofs,
inventories, archived work replay and actual library parser/summary output. A
replaced manifest, failed parent or incomplete child cannot bypass admission.

```rust,ignore
let candidate = import_job::Candidate::read(candidate_file)?;
let mut request = import_job::Resume::new(candidate);
request.duplicate_policy = chosen_policy;
let result = import_job::resume(&request, &mut store, &cancel, &mut progress)?;
```

`Resume` JSON contains `candidate`, `duplicatePolicy` (default reject) and `wallMs`
(default 600000). Use a fresh `NetworkCancel` for each resumed attempt; tokens
remain cancelled once triggered. Resume needs no worker, network/cache services, font installation or
new extraction. Cancellation, duplicate conflict, stale planning or staging failure
after successful preparation retains the candidate for this operation. Bind an
existing Ready directory explicitly to use it through this API, including older
candidate outputs created before this coordinator existed.

The token is created exclusively, its file is synced, then its directory is
synced. A failed/interrupted token write never means installation. A partial token
fails parsing; the host may explicitly rebind the unchanged outer Ready manifest
to a new token. Preparation checkpoint survival through actual power loss is not
certified: missing/changed source evidence will fail revalidation. Only the
managed store owns installed-state durability. There is no separate job journal
that can override the authoritative library catalog.

## Progress and terminal receipt

`Progress` serializes with a `stage` tag:

- `preparing`: optional nested URL/preparation progress. A nested `ready` only
  means preparation readiness, never library installation.
- `validating`: optional admission progress (receipts, inventory, replay, library,
  complete). No library mutation has started.
- `planned`: a `review` object containing `candidate`, the full admitted `import`
  summary and the chosen host `plan`. It exposes song entries, diagnostics,
  provisional musical status and duplicate decisions before any transaction.
- `installing`: nested host begin/evidence/songs/ready-to-commit progress.

Progress callbacks may return an error. The cancel token and wall limit are
checked before and after each callback. A callback can stop at `planned`, let the
host show that review, and later resume with its chosen policy. This is an
available host workflow, not a mandatory approval dialog. Identical revisions are
no-ops; changed revisions default to a conflict; explicit replacement still cannot
remove installed selectors. Stale plans are returned for replanning, never retried
automatically.

There is intentionally no fallible final progress callback. The SDK returns:

```text
{
  schema: "ddi-danoni-import-job-result/1",
  review: {candidate, import: prepared_import::Summary, plan: PlanSummary},
  installation: import_host::Outcome
}
```

The full review/result buffers are constructed before installation. No filesystem
write, cancellation/deadline check or caller callback follows `import_host::install`.
A cancellation arriving inside/after commit cannot turn installed content into a
cancelled job. The caller receives installed counts/revision, unchanged revision,
or `needs-recovery` with its transaction/import identity. The host can serialize
this receipt, update its UI or save an import history; a failure in those later
notifications must not relabel a committed library operation.

Uncertain commits are neither rolled back nor retried. Run the target store's
explicit recovery API and reconcile its catalog first, then decide whether a
fresh resume is needed. A successful same-content resume will usually be a no-op.
Errors during known uncommitted staging use the existing rollback contract;
`library-rollback-failed` and failed transaction initialization can still leave
recovery markers. The coordinator does not infer store cleanup from a generic
error or from a candidate directory's presence.

## CLI

```sh
BIN=target/danoni-rust/debug/ddi-danoni-export
$BIN create-store /external/new-library
$BIN import-url-native /external/trusted-worker-bundle \
  /external/import-job.json /external/new-candidate /external/new-library
$BIN resume-import /external/new-candidate/import-candidate.json \
  /external/new-library
$BIN bind-import /external/older-ready-candidate > /external/candidate.json
$BIN resume-import /external/candidate.json /external/new-library replace
```

`import-url-native` reads a job with schema `ddi-danoni-import-job/1`, `cache`,
`roots` (default empty), `origins` and `request` as above. Paths resolve relative
to the job file; store and candidate paths are explicit command arguments.
`origins` grants no robots exemption and uses the existing reviewed public-access
policy. `request.preparation.source.fetch.offline:true` requires complete cached
inputs. The worker is verified from the host-selected bundle. Songs cannot choose
the bundle, filesystem namespaces, target store or site decisions.

`resume-import` defaults to reject and accepts optional `reject`, `keep-existing`
or `replace`. These commands print the serializable result to stdout; they do not
write a post-commit result file. Keep the candidate token and inspect/recover the
store if an invoking shell loses that output. No source scripts are executed by
shell/Python/Node, and no new browser dependency is introduced.

## Acceptance and remaining work

Synthetic tests cover restart tokens, exact Ready changes/failed parents,
pre-cancelled jobs, eight phase cancellations, callback errors, deadlines,
entered-runtime refusal, duplicate policies and selector coverage. Fake backend
commits reject or become uncertain before/after publication, while cancelling the
job at commit; the coordinator must issue no terminal callback, rollback or retry
for committed/uncertain outcomes. Source-preparation failure leaves the library
untouched. Existing managed-store tests supply the actual filesystem crash matrix.

The optional `import_job_corpus` test requires `DDI_DANONI_IMPORT_URL_JOB`,
`DDI_DANONI_WORKER_BUNDLE` and `DDI_DANONI_IMPORT_JOB_ROOT` (nonexistent external
folder). It executes an offline URL job, cancels at `planned`, reopens the library,
resumes from the persisted token with no preparation services, verifies installed
assets and checks a second resume is unchanged. `DDI_DANONI_IMPORT_EXPECTED_READY`
optionally compares the resulting content digest to a frozen earlier candidate.
Without those variables it skips external work. Third-party inputs/results stay
local; only synthetic fixtures are checked in.

Desktop UI/task scheduling, playback asset integration, import history/quotas and
retention remain main-shell work. Broader URL traversal, HTML5 and native engine
families retain their existing admission boundaries. This slice adds an embeddable
job seam, not a new accepted musical corpus or main-app desktop implementation.


The 2026-10-15 local checkpoint is under
`~/Music/DanOni/reviews/rust-import-job-20261015/`. Higan's final offline API job
cancels at `planned` with no library writes, then reopens and installs from the
saved token without preparing again; another resume is unchanged. The final CLI
imports No Name/灼冬赫夜 from its URL into that same fresh review store, yielding
two songs / three charts. Seven CLI checks cover URL import, old/new candidate
binding/resume, exact-hash rejection, frozen-candidate comparison and full store
verification. A changed token leaves the catalog byte-identical.

Both source runs issue zero network requests and match the admitted content
digests from their earlier frozen Ready outputs. Their BPM, preview, artwork,
ratings, timing and native-motion decisions are unchanged and still provisional.
This proves the joined SDK/CLI/restart path for those supported works, not new
musical acceptance or broader URL/engine compatibility. Raw sources, caches,
candidates, installed files and reports remain local.

Final checks pass 243 native / 94 feature-disabled SDK tests, strict Clippy in
both configurations, formatting and `UV_OFFLINE=1 just check` (850/850 REUSE).
The native suite runs the offline URL job plus historical cache/page/import/store
oracles. The added non-UTF-8 path regression rejects at the API boundary without
requiring the filesystem to accept an invalid name. Local `validation.json` binds
final source/lockfile/binary hashes, reports, logs and unchanged original export,
registry and cache indexes. Earlier exploratory runs and the macOS-invalid test
setup failure are retained in separate logs, not treated as passing final checks.
