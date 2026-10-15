//! Thin CLI configuration adapters; hosts can call discovery/execution directly.
use super::*;
use crate::{
    assets::{self, AssetSource, JSON_LIMIT},
    import_job,
    managed_store::Store,
    native,
    network::NetworkCancel,
};
use std::path::{Path, PathBuf};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job<R> {
    schema: String,
    #[serde(default)]
    roots: BTreeMap<String, PathBuf>,
    cache: PathBuf,
    origins: BTreeMap<String, String>,
    request: R,
}
fn read<R: serde::de::DeserializeOwned>(path: &Path, schema: &str) -> Result<(Job<R>, PathBuf)> {
    let path = path.canonicalize().map_err(native::io)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Job parent"))?
        .to_path_buf();
    let source = native::DirectorySource::new(BTreeMap::from([("job".into(), parent.clone())]))?;
    let mut job: Job<R> = assets::json(
        &source.read(
            "job",
            path.file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| invalid("Job filename"))?,
            JSON_LIMIT,
        )?,
    )?;
    if job.schema != schema {
        return Err(invalid("Unsupported collection job schema"));
    }
    if !job.cache.is_absolute() {
        job.cache = parent.join(&job.cache);
    }
    Ok((job, parent))
}
fn decision(origins: &BTreeMap<String, String>, url: &str) -> Result<String> {
    let origin = url::Url::parse(url)
        .map_err(|e| invalid(e.to_string()))?
        .origin()
        .ascii_serialization();
    origins.get(&origin).cloned().ok_or_else(|| {
        crate::Error::new(
            "site-terms-denied",
            "Origin has no host-reviewed public-access decision",
        )
    })
}
pub fn discover_job(job: &Path, output: &Path) -> Result<Inventory> {
    let (job, _) = read::<IndexRequest>(job, "ddi-danoni-index-job/1")?;
    let discovery = discover(
        &job.request,
        &job.cache,
        output,
        &NetworkCancel::default(),
        &mut |url| decision(&job.origins, url),
        &mut |_| Ok(()),
    )?;
    Ok(discovery.inventory)
}
pub fn execute_job(
    bundle: &Path,
    job: &Path,
    discovery: Option<&Path>,
    output: &Path,
    store: &Path,
) -> Result<Outcome> {
    let (job, parent) = read::<Request>(job, "ddi-danoni-collection-job/1")?;
    let assets = native::DirectorySource::new(
        job.roots
            .into_iter()
            .map(|(k, p)| (k, if p.is_absolute() { p } else { parent.join(p) }))
            .collect(),
    )?;
    let worker = crate::worker_build::load_bundle(bundle, &mut || Ok(()))?;
    let mut store = Store::open(store)?;
    let services = import_job::Services {
        cache: &job.cache,
        worker: &worker,
        assets: &assets,
    };
    let cancel = NetworkCancel::default();
    let mut site = |url: &str| decision(&job.origins, url);
    match discovery {
        Some(path) => execute_discovered(
            &job.request,
            &load_discovery(path)?,
            &services,
            output,
            &mut store,
            &cancel,
            &mut site,
            &mut |_| Ok(()),
        ),
        None => execute(
            &job.request,
            &services,
            output,
            &mut store,
            &cancel,
            &mut site,
            &mut |_| Ok(()),
        ),
    }
}
