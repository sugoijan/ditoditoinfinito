//! Sequential child jobs. Library publication is atomic per work, never per list.
use super::*;
use crate::{
    assets::{self, AssetSource, JSON_LIMIT},
    import_host, import_job, native,
    network::NetworkCancel,
    orchestration,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn works() -> usize {
    16
}
fn wall() -> u64 {
    900000
}
fn bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub index: IndexRequest,
    /// URL is replaced with each exact discovered destination. A global movie
    /// override is forbidden; actual work metadata and selector checks still win.
    pub work: import_job::Request,
    pub selected: Option<Vec<usize>>,
    #[serde(default = "works")]
    pub maximum_works: usize,
    #[serde(default = "wall")]
    pub wall_ms: u64,
    #[serde(default = "bytes")]
    pub output_bytes: u64,
}
impl Request {
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            index: IndexRequest::new(url.clone()),
            work: import_job::Request::new(url),
            selected: None,
            maximum_works: works(),
            wall_ms: wall(),
            output_bytes: bytes(),
        }
    }
    fn check(&self) -> Result<()> {
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(invalid("Call collection imports on a blocking job thread"));
        }
        Route::recognize(&SourceAddress::parse(&self.index.url)?)?;
        if !(1..=64).contains(&self.maximum_works)
            || self.wall_ms == 0
            || self.wall_ms > 3600000
            || self.output_bytes == 0
            || self.output_bytes > 4 * 1024 * 1024 * 1024
            || self.work.preparation.source.movie_url.is_some()
        {
            return Err(invalid(
                "Require maximumWorks 1–64, wallMs 1–3600000, outputBytes 1–4 GiB and no global movieUrl",
            ));
        }
        // Saved discovery execution does not open an index Fetcher. Validate its
        // shared budgets here too, before allowing any child/library operation.
        let f = &self.index.fetch;
        if f.maximum_requests == 0
            || f.maximum_requests > 64
            || f.maximum_wire_bytes == 0
            || f.maximum_wire_bytes > 512 * 1024 * 1024
            || f.maximum_decoded_bytes == 0
            || f.maximum_decoded_bytes > 512 * 1024 * 1024
            || f.wall_ms == 0
            || f.wall_ms > 300000
            || f.robots_max_age_seconds == 0
            || f.robots_max_age_seconds > 86400
            || !(1..=60000).contains(&f.transport.request_timeout_ms)
        {
            return Err(invalid("Invalid shared fetch bounds"));
        }
        Ok(())
    }
}
#[derive(Debug, Serialize)]
#[serde(
    tag = "stage",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Progress<'a> {
    Discovering {
        detail: &'a DiscoveryProgress,
    },
    Discovered {
        inventory: &'a Inventory,
    },
    Work {
        entry: usize,
        detail: &'a import_job::Progress,
    },
}
#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum EntryResult {
    NotSelected,
    NotAttempted,
    Skipped { reason: String },
    Failed { error: Error },
    Completed { outcome: Box<import_job::Outcome> },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkResult {
    pub entry: usize,
    pub id: String,
    pub label: String,
    pub url: Option<String>,
    pub result: EntryResult,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub schema: &'static str,
    pub request: Request,
    pub directory: PathBuf,
    pub discovery_directory: PathBuf,
    pub discovery_sha256: String,
    pub complete: bool,
    pub entries: Vec<WorkResult>,
    /// Counts for this execution only; a reused discovery has its own receipt.
    pub transfers: Counts,
    pub accounting_complete: bool,
    pub stop: Option<Error>,
    /// A report write failure cannot turn a committed child into a failed child.
    pub report_error: Option<Error>,
}
struct Control<'a> {
    request: &'a Request,
    cancel: &'a NetworkCancel,
    started: Instant,
    last_size: Instant,
    root: &'a Path,
}
impl Control<'_> {
    fn remaining(&self) -> Result<u64> {
        if self.cancel.is_cancelled() {
            return Err(Error::new(
                "request-cancelled",
                "Collection cancelled; earlier child results are retained",
            ));
        }
        self.request
            .wall_ms
            .checked_sub(self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::new("collection-timeout", "Whole collection deadline exceeded"))
    }
    fn check(&mut self, force: bool) -> Result<()> {
        self.remaining()?;
        if force || self.last_size.elapsed() >= Duration::from_millis(50) {
            if orchestration::tree_size(self.root)? > self.request.output_bytes {
                return Err(Error::new(
                    "collection-output-limit",
                    "Whole collection output limit exceeded",
                ));
            }
            self.last_size = Instant::now();
        }
        Ok(())
    }
}
fn selection(request: &Request, inventory: &Inventory) -> Result<BTreeSet<usize>> {
    let chosen = request
        .selected
        .clone()
        .unwrap_or_else(|| (0..inventory.entries.len()).collect());
    let set = chosen.iter().copied().collect::<BTreeSet<_>>();
    if set.is_empty()
        || set.len() != chosen.len()
        || set.iter().any(|n| *n >= inventory.entries.len())
    {
        return Err(invalid(
            "selected must contain unique, in-range inventory indices",
        ));
    }
    Ok(set)
}
fn clip(
    request: &Request,
    used: Counts,
    url: &str,
    remaining: u64,
    output_remaining: u64,
) -> Result<import_job::Request> {
    let global = &request.index.fetch;
    let nr = global
        .maximum_requests
        .checked_sub(used.requests)
        .ok_or_else(|| invalid("Request accounting exceeds shared budget"))?;
    let nw = global
        .maximum_wire_bytes
        .checked_sub(used.wire_bytes)
        .ok_or_else(|| invalid("Wire accounting exceeds shared budget"))?;
    let nd = global
        .maximum_decoded_bytes
        .checked_sub(used.decoded_bytes)
        .ok_or_else(|| invalid("Decoded accounting exceeds shared budget"))?;
    let mut job = request.work.clone();
    job.preparation.source.url = url.into();
    let f = &mut job.preparation.source.fetch;
    f.offline |= global.offline || nr == 0 || nw == 0 || nd == 0;
    f.maximum_requests = f.maximum_requests.min(nr.max(1));
    f.maximum_wire_bytes = f.maximum_wire_bytes.min(nw.max(1));
    f.maximum_decoded_bytes = f.maximum_decoded_bytes.min(nd.max(1));
    f.transport = global.transport.clone();
    f.robots_max_age_seconds = f.robots_max_age_seconds.min(global.robots_max_age_seconds);
    f.wall_ms = f.wall_ms.min(remaining).min(global.wall_ms);
    job.wall_ms = job.wall_ms.min(remaining).min(900000);
    job.preparation.wall_ms = job.preparation.wall_ms.min(remaining);
    job.preparation.output_bytes = job.preparation.output_bytes.min(output_remaining.max(1));
    Ok(job)
}
fn receipt(root: &Path, job: &import_job::Request) -> Result<Counts> {
    let source =
        native::DirectorySource::new(BTreeMap::from([("child".into(), root.to_path_buf())]))?;
    let paths = ["source/manifest.json", "source/failure.json"];
    let existing = paths
        .iter()
        .filter(|p| root.join(p).exists())
        .collect::<Vec<_>>();
    if existing.len() != 1 {
        return Err(Error::new(
            "collection-accounting-incomplete",
            "Child has no unique source transfer receipt; remaining jobs stopped",
        ));
    }
    let m: Value = assets::json(&source.read("child", existing[0], JSON_LIMIT)?)?;
    if m["schema"] != "ddi-danoni-url-native-run/1"
        || m["request"]
            != serde_json::to_value(&job.preparation.source).map_err(|e| invalid(e.to_string()))?
    {
        return Err(invalid(
            "Child transfer receipt belongs to another source request",
        ));
    }
    let c: Counts =
        serde_json::from_value(m["transfers"].clone()).map_err(|e| invalid(e.to_string()))?;
    let f = &job.preparation.source.fetch;
    if c.requests > f.maximum_requests
        || c.wire_bytes > f.maximum_wire_bytes
        || c.decoded_bytes > f.maximum_decoded_bytes
        || (f.offline && (c.requests != 0 || c.wire_bytes != 0 || c.decoded_bytes != 0))
    {
        return Err(invalid("Child exceeds clipped transfer budget"));
    }
    Ok(c)
}
fn add(a: Counts, b: Counts) -> Result<Counts> {
    Ok(Counts {
        requests: a
            .requests
            .checked_add(b.requests)
            .ok_or_else(|| invalid("Count overflow"))?,
        wire_bytes: a
            .wire_bytes
            .checked_add(b.wire_bytes)
            .ok_or_else(|| invalid("Count overflow"))?,
        decoded_bytes: a
            .decoded_bytes
            .checked_add(b.decoded_bytes)
            .ok_or_else(|| invalid("Count overflow"))?,
    })
}
fn save(out: &Outcome) -> Result<()> {
    orchestration::atomic(
        &out.directory,
        "manifest.json",
        &serde_json::to_value(out).map_err(|e| invalid(e.to_string()))?,
    )
}
// The runner is private: tests can inject transaction outcomes without executing
// a movie; production always calls the existing reviewed single-work coordinator.
#[allow(clippy::too_many_arguments)]
fn run_entries(
    request: &Request,
    discovery: &Discovery,
    root: &Path,
    initial: Counts,
    started: Instant,
    cancel: &NetworkCancel,
    progress: &mut impl FnMut(&Progress<'_>) -> Result<()>,
    runner: &mut impl FnMut(
        &import_job::Request,
        &Path,
        &mut dyn FnMut(&import_job::Progress) -> Result<()>,
    ) -> Result<import_job::Outcome>,
) -> Result<Outcome> {
    let selected = selection(request, &discovery.inventory)?;
    let mut out = Outcome {
        schema: "ddi-danoni-collection-result/1",
        request: request.clone(),
        directory: root.to_path_buf(),
        discovery_directory: discovery.root.clone(),
        discovery_sha256: discovery.manifest_sha256.clone(),
        complete: false,
        entries: discovery
            .inventory
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| WorkResult {
                entry: i,
                id: e.id.clone(),
                label: e.label.clone(),
                url: e.address.as_ref().map(|a| a.retrieval_url.clone()),
                result: if !selected.contains(&i) {
                    EntryResult::NotSelected
                } else if e.kind != EntryKind::Candidate {
                    EntryResult::Skipped {
                        reason: e
                            .reason
                            .clone()
                            .unwrap_or_else(|| "No native candidate".into()),
                    }
                } else {
                    EntryResult::NotAttempted
                },
            })
            .collect(),
        transfers: initial,
        accounting_complete: true,
        stop: None,
        report_error: None,
    };
    let mut control = Control {
        request,
        cancel,
        started,
        last_size: Instant::now(),
        root,
    };
    control.check(true)?;
    progress(&Progress::Discovered {
        inventory: &discovery.inventory,
    })?;
    control.check(true)?;
    save(&out)?; // Before any child can publish.
    let mut attempts = 0;
    for i in selected {
        if !matches!(out.entries[i].result, EntryResult::NotAttempted) {
            continue;
        }
        if attempts == request.maximum_works {
            out.stop = Some(Error::new(
                "collection-work-limit",
                "maximumWorks reached; remaining entries were not attempted",
            ));
            break;
        }
        let result = (|| {
            control.check(true)?;
            let remaining = control.remaining()?;
            let size = orchestration::tree_size(root)?;
            let available = request
                .output_bytes
                .checked_sub(size)
                .filter(|n| *n > 0)
                .ok_or_else(|| Error::new("collection-output-limit", "No output budget remains"))?;
            clip(
                request,
                out.transfers,
                out.entries[i]
                    .url
                    .as_deref()
                    .ok_or_else(|| invalid("Candidate has no URL"))?,
                remaining,
                available,
            )
        })();
        let job = match result {
            Ok(j) => j,
            Err(e) => {
                out.stop = Some(e);
                break;
            }
        };
        attempts += 1;
        let child = root.join(format!("work-{i:04}"));
        let mut callback_error = None;
        let mut callback = |p: &import_job::Progress| {
            let result = (|| {
                control.check(false)?;
                progress(&Progress::Work {
                    entry: i,
                    detail: p,
                })?;
                control.check(false)
            })();
            if let Err(e) = &result {
                callback_error = Some(e.clone());
            }
            result
        };
        let result = runner(&job, &child, &mut callback);
        // Record the publication result before any post-child I/O/check/callback.
        let needs_recovery = matches!(&result,Ok(v) if matches!(v.installation,import_host::Outcome::NeedsRecovery{..}));
        let failed = result.as_ref().err().cloned();
        out.entries[i].result = match result {
            Ok(v) => EntryResult::Completed {
                outcome: Box::new(v),
            },
            Err(e) => EntryResult::Failed { error: e },
        };
        match receipt(&child, &job).and_then(|c| add(out.transfers, c)) {
            Ok(c) => out.transfers = c,
            Err(e) => {
                out.accounting_complete = false;
                out.stop = Some(e);
            }
        }
        if let Some(e) = callback_error {
            out.stop = Some(e);
        }
        if needs_recovery {
            out.stop = Some(Error::new(
                "library-needs-recovery",
                "Child publication uncertain; explicit store recovery required",
            ));
        }
        if let Some(e) = failed
            && [
                "library-needs-recovery",
                "library-rollback-failed",
                "request-cancelled",
            ]
            .contains(&e.code)
        {
            out.stop = Some(e);
        }
        if let Err(e) = save(&out) {
            out.report_error = Some(e);
            break;
        }
        if out.stop.is_some() {
            break;
        }
    }
    out.complete = discovery.inventory.complete
        && out.stop.is_none()
        && out.accounting_complete
        && out.entries.iter().all(|e| match &e.result {
            EntryResult::NotSelected => true,
            EntryResult::Completed { outcome } => !matches!(
                outcome.installation,
                import_host::Outcome::NeedsRecovery { .. }
            ),
            _ => false,
        });
    if out.report_error.is_none()
        && let Err(e) = save(&out)
    {
        out.report_error = Some(e);
    }
    // No final cancellation/progress callback: completed child commits stand.
    Ok(out)
}
fn fresh(output: &Path) -> Result<orchestration::Owned> {
    let mut o = orchestration::new_root(output)?;
    if o.root.to_str().is_none() {
        return Err(invalid("Collection root must be UTF-8"));
    }
    o.retain = true;
    Ok(o)
}
#[allow(clippy::too_many_arguments)]
fn execute_inner(
    request: &Request,
    discovery: &Discovery,
    services: &import_job::Services<'_, impl AssetSource>,
    output: &Path,
    store: &mut impl import_job::CatalogStore,
    cancel: &NetworkCancel,
    site: &mut impl FnMut(&str) -> Result<String>,
    progress: &mut impl FnMut(&Progress<'_>) -> Result<()>,
    initial: Counts,
    started: Instant,
) -> Result<Outcome> {
    run_entries(
        request,
        discovery,
        output,
        initial,
        started,
        cancel,
        progress,
        &mut |job, child, callback| {
            store.snapshot()?; // Refuse a blocked catalog before source execution.
            import_job::execute(job, services, child, store, cancel, site, &mut |p| {
                callback(p)
            })
        },
    )
}
/// Discover one reviewed list, then execute selected candidates sequentially.
/// An individual failure is partial; uncertain publication stops all later work.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    request: &Request,
    services: &import_job::Services<'_, impl AssetSource>,
    output: &Path,
    store: &mut impl import_job::CatalogStore,
    cancel: &NetworkCancel,
    site: &mut impl FnMut(&str) -> Result<String>,
    progress: &mut impl FnMut(&Progress<'_>) -> Result<()>,
) -> Result<Outcome> {
    request.check()?;
    let started = Instant::now();
    let o = fresh(output)?;
    let mut index = request.index.clone();
    index.fetch.wall_ms = index.fetch.wall_ms.min(request.wall_ms);
    let discovery = discover(
        &index,
        services.cache,
        &o.root.join("index"),
        cancel,
        site,
        &mut |p| {
            if started.elapsed().as_millis() >= u128::from(request.wall_ms)
                || orchestration::tree_size(&o.root)? > request.output_bytes
            {
                return Err(Error::new(
                    "collection-limit",
                    "Collection discovery budget exceeded",
                ));
            }
            progress(&Progress::Discovering { detail: p })
        },
    )?;
    execute_inner(
        request,
        &discovery,
        services,
        &o.root,
        store,
        cancel,
        site,
        progress,
        discovery.transfers,
        started,
    )
}
/// Use an opaque, replay-validated discovery without fetching its index again.
/// This creates a fresh attempt; per-child Ready tokens remain the resume API.
#[allow(clippy::too_many_arguments)]
pub fn execute_discovered(
    request: &Request,
    discovery: &Discovery,
    services: &import_job::Services<'_, impl AssetSource>,
    output: &Path,
    store: &mut impl import_job::CatalogStore,
    cancel: &NetworkCancel,
    site: &mut impl FnMut(&str) -> Result<String>,
    progress: &mut impl FnMut(&Progress<'_>) -> Result<()>,
) -> Result<Outcome> {
    request.check()?;
    if SourceAddress::parse(&request.index.url)?.retrieval_url
        != SourceAddress::parse(&discovery.request.url)?.retrieval_url
        || request.index.maximum_entries != discovery.request.maximum_entries
        || request.index.encoding_override != discovery.request.encoding_override
    {
        return Err(invalid(
            "Execution must use the saved discovery URL, encoding and entry bound",
        ));
    }
    selection(request, &discovery.inventory)?;
    let o = fresh(output)?;
    execute_inner(
        request,
        discovery,
        services,
        &o.root,
        store,
        cancel,
        site,
        progress,
        Counts::default(),
        Instant::now(),
    )
}
#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
