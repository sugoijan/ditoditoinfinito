use super::*;
use crate::{
    assets::{self, *},
    native,
    network::{
        FetchOptions, Fetcher, NetworkCancel, ReplayRequest, RequestContext, TransferCounts,
    },
    orchestration,
    url_native::evidence::Evidence,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
fn entries() -> usize {
    512
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexRequest {
    pub url: String,
    pub encoding_override: Option<String>,
    #[serde(default = "entries")]
    pub maximum_entries: usize,
    #[serde(default)]
    pub fetch: FetchOptions,
}
impl IndexRequest {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            encoding_override: None,
            maximum_entries: entries(),
            fetch: FetchOptions::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Counts {
    pub requests: usize,
    pub wire_bytes: u64,
    pub decoded_bytes: u64,
}
impl From<&TransferCounts> for Counts {
    fn from(c: &TransferCounts) -> Self {
        Self {
            requests: c.requests,
            wire_bytes: c.wire_bytes,
            decoded_bytes: c.decoded_bytes,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(
    tag = "stage",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum DiscoveryProgress {
    Fetching { url: String },
    Parsed { entries: usize, complete: bool },
    Ready,
}
/// Only `discover`/`load_discovery` construct an executable discovery. Its saved
/// inventory alone is informational; loading replays it against exact HTML bytes.
pub struct Discovery {
    pub(crate) root: PathBuf,
    pub(crate) request: IndexRequest,
    pub(crate) inventory: Inventory,
    pub(crate) transfers: Counts,
    pub(crate) manifest_sha256: String,
}
impl Discovery {
    pub fn request(&self) -> &IndexRequest {
        &self.request
    }
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }
    pub fn directory(&self) -> &Path {
        &self.root
    }
    pub fn transfers(&self) -> Counts {
        self.transfers
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
}
fn check(request: &IndexRequest, cancel: &NetworkCancel, started: Instant) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(Error::new("request-cancelled", "Index discovery cancelled"));
    }
    if started.elapsed().as_millis() >= u128::from(request.fetch.wall_ms) {
        return Err(Error::new(
            "index-timeout",
            "Index discovery deadline exceeded",
        ));
    }
    Ok(())
}
fn address_guard(initial: &SourceAddress, next: &str) -> Result<()> {
    let a = SourceAddress::parse(next)?;
    if initial.capture.is_some() && a.capture.is_none() {
        return Err(Error::new(
            "archive-live-fallback",
            "Index redirects cannot silently leave the archive",
        ));
    }
    if Route::recognize(initial)? != Route::recognize(&a)?
        || url::Url::parse(&initial.logical_url)
            .ok()
            .map(|u| u.path().to_owned())
            != url::Url::parse(&a.logical_url)
                .ok()
                .map(|u| u.path().to_owned())
    {
        return Err(Error::new(
            "index-redirect-route",
            "Redirect leaves the reviewed index route",
        ));
    }
    Ok(())
}
/// Fetch exactly one reviewed index (and explicit redirects/robots), not child
/// pages. The existing fetcher owns cache rules, pacing, DNS and site decisions.
#[allow(clippy::too_many_arguments)]
pub fn discover(
    request: &IndexRequest,
    cache: &Path,
    output: &Path,
    cancel: &NetworkCancel,
    site: &mut impl FnMut(&str) -> Result<String>,
    progress: &mut impl FnMut(&DiscoveryProgress) -> Result<()>,
) -> Result<Discovery> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(invalid("Call index discovery on a blocking job thread"));
    }
    let initial = SourceAddress::parse(&request.url)?;
    Route::recognize(&initial)?;
    if !(1..=1024).contains(&request.maximum_entries) {
        return Err(invalid("maximumEntries must be 1–1024"));
    }
    let started = Instant::now();
    check(request, cancel, started)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(native::io)?;
    let mut fetcher = Fetcher::open(cache, request.fetch.clone())?;
    let mut owned = orchestration::new_root(output)?;
    if owned.root.to_str().is_none() {
        return Err(invalid("Discovery root must be UTF-8"));
    }
    // Preserve failed acquisition diagnostics too. A directory is never Ready.
    owned.retain = true;
    let root = &owned.root;
    let evidence = Evidence::default();
    let result = (|| {
        let mut emit = |p: DiscoveryProgress| {
            check(request, cancel, started)?;
            progress(&p)?;
            check(request, cancel, started)
        };
        emit(DiscoveryProgress::Fetching {
            url: initial.retrieval_url.clone(),
        })?;
        let resource=runtime.block_on(async{
            let req=ReplayRequest{url:initial.retrieval_url.clone(),context:RequestContext::default(),maximum_bytes:acquisition::PAGE_LIMIT};
            let mut guard=|url:&str|address_guard(&initial,url);
            let mut future=Box::pin(fetcher.fetch_checked(&req,cancel,site,&mut guard));let mut timer=tokio::time::interval(std::time::Duration::from_millis(50));
            loop {tokio::select!{biased;
                _=timer.tick()=>{if let Err(e)=emit(DiscoveryProgress::Fetching{url:initial.retrieval_url.clone()}){cancel.cancel();let _=future.await;return Err(e);}},
                result=&mut future=>return result,
            }}
        })?;
        let http = evidence.archive(root, &fetcher, &resource)?;
        let response = resource.response();
        if response.status != 200 {
            return Err(Error::new(
                "index-unavailable",
                format!("Index returned HTTP {}", response.status),
            ));
        }
        if response.selected_response["mimeType"] != "text/html" {
            return Err(Error::new(
                "unsupported-index-content",
                "Reviewed index must return HTML",
            ));
        }
        address_guard(&initial, &response.response_url)?;
        let hint = request
            .encoding_override
            .as_deref()
            .or_else(|| response.selected_response["charset"].as_str());
        let inventory = inspect(
            &response.response_url,
            &response.body,
            hint,
            request.maximum_entries,
        )?;
        emit(DiscoveryProgress::Parsed {
            entries: inventory.entries.len(),
            complete: inventory.complete,
        })?;
        let inventory_binding = evidence.json(root, "inventory.json".into(), &inventory)?;
        let manifest = json!({"schema":"ddi-danoni-index-discovery-run/1","ready":true,"request":request,"http":http,"body":evidence.body(&response.body_binding),"inventory":inventory_binding,"inputs":evidence.bindings()?,"transfers":Counts::from(fetcher.counts())});
        // Complete bytes are staged before the last cancellable checkpoint.
        orchestration::atomic(root, "pending.json", &manifest)?;
        emit(DiscoveryProgress::Ready)?;
        std::fs::rename(root.join("pending.json"), root.join("manifest.json"))
            .map_err(native::io)?;
        Ok(())
    })();
    if let Err(e) = result {
        let failure = json!({"schema":"ddi-danoni-index-discovery-run/1","ready":false,"request":request,"failure":e,"inputs":evidence.bindings()?,"transfers":Counts::from(fetcher.counts())});
        orchestration::atomic(root, "failure.json", &failure)?;
        return Err(e);
    }
    drop(fetcher);
    load_discovery(root)
}
/// Read-only proof replay; neither cache access nor HTTP permission is inferred
/// from saved index evidence. Every copied HTTP/body binding is checked first.
pub fn load_discovery(root: &Path) -> Result<Discovery> {
    let root = root.canonicalize().map_err(native::io)?;
    if root.to_str().is_none() {
        return Err(invalid("Discovery root must be UTF-8"));
    }
    let source = native::DirectorySource::new(BTreeMap::from([("url".into(), root.clone())]))?;
    let body = source.read("url", "manifest.json", JSON_LIMIT)?;
    let m: Value = assets::json(&body)?;
    if m["schema"] != "ddi-danoni-index-discovery-run/1" || m["ready"] != true {
        return Err(invalid("Not a Ready index discovery"));
    }
    let request: IndexRequest =
        serde_json::from_value(m["request"].clone()).map_err(|e| invalid(e.to_string()))?;
    let initial = SourceAddress::parse(&request.url)?;
    Route::recognize(&initial)?;
    let mut inputs = Inputs::new(&source);
    let mut seen = BTreeMap::new();
    let bindings = m["inputs"]
        .as_array()
        .filter(|a| a.len() <= 4096)
        .ok_or_else(|| invalid("Index input inventory"))?;
    for row in bindings {
        let b: Binding = serde_json::from_value(row.clone()).map_err(|e| invalid(e.to_string()))?;
        if b.namespace != "url" || seen.insert(b.path.clone(), b.sha256.clone()).is_some() {
            return Err(invalid("Repeated/foreign index input"));
        }
        inputs.checked(&b, MEDIA_LIMIT)?;
    }
    let mut get = |row: &Value, max| -> Result<std::sync::Arc<Vec<u8>>> {
        let b: Binding = serde_json::from_value(row.clone()).map_err(|e| invalid(e.to_string()))?;
        if b.namespace != "url" || seen.get(&b.path) != Some(&b.sha256) {
            return Err(invalid("Unbound index artifact"));
        }
        inputs.checked(&b, max)
    };
    let http: Value = assets::json(&get(&m["http"], JSON_LIMIT)?)?;
    let response = http["responses"]
        .as_array()
        .and_then(|a| a.last())
        .ok_or_else(|| invalid("Missing index HTTP chain"))?;
    if http["schema"] != "ddi-danoni-http-resource/1"
        || http["requestedUrl"] != initial.retrieval_url
        || response["status"] != 200
        || response["selectedResponse"]["mimeType"] != "text/html"
        || response["selectedResponse"]["completeBody"] != true
    {
        return Err(invalid("Incomplete index HTTP evidence"));
    }
    let response_url = text(response, "responseUrl")?;
    address_guard(&initial, response_url)?;
    let raw = &response["body"];
    if raw["namespace"] != "cache"
        || m["body"]["path"] != format!("cache/{}", text(raw, "path")?)
        || m["body"]["sha256"] != raw["sha256"]
    {
        return Err(invalid("Index response/body binding differs"));
    }
    let html = get(&m["body"], acquisition::PAGE_LIMIT)?;
    if response["byteLength"] != html.len() {
        return Err(invalid("Index body length differs"));
    }
    let hint = request
        .encoding_override
        .as_deref()
        .or_else(|| response["selectedResponse"]["charset"].as_str());
    let inventory = inspect(response_url, &html, hint, request.maximum_entries)?;
    let recorded: Value = assets::json(&get(&m["inventory"], JSON_LIMIT)?)?;
    if recorded != serde_json::to_value(&inventory).map_err(|e| invalid(e.to_string()))? {
        return Err(invalid("Index inventory does not replay from bound HTML"));
    }
    let transfers: Counts =
        serde_json::from_value(m["transfers"].clone()).map_err(|e| invalid(e.to_string()))?;
    if transfers.requests > request.fetch.maximum_requests
        || transfers.wire_bytes > request.fetch.maximum_wire_bytes
        || transfers.decoded_bytes > request.fetch.maximum_decoded_bytes
    {
        return Err(invalid("Discovery exceeds declared transfer bounds"));
    }
    Ok(Discovery {
        root,
        request,
        inventory,
        transfers,
        manifest_sha256: sha256(&body),
    })
}
