use super::*;
use super::{
    store::{CacheStore, stamp},
    transport::HttpTransport,
};
use crate::{
    Error, Result,
    assets::{MEDIA_LIMIT, sha256},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::time::Instant;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct FetchOptions {
    pub offline: bool,
    pub transport: TransportOptions,
    pub maximum_requests: usize,
    pub maximum_wire_bytes: u64,
    pub maximum_decoded_bytes: u64,
    pub wall_ms: u64,
    pub robots_max_age_seconds: u64,
}
impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            offline: false,
            transport: TransportOptions::default(),
            maximum_requests: 64,
            maximum_wire_bytes: 256 * 1024 * 1024,
            maximum_decoded_bytes: 256 * 1024 * 1024,
            wall_ms: 180000,
            robots_max_age_seconds: 86400,
        }
    }
}
impl FetchOptions {
    fn validate(&self) -> Result<()> {
        if !(1..=60000).contains(&self.transport.request_timeout_ms)
            || self.maximum_requests == 0
            || self.maximum_requests > 64
            || self.maximum_wire_bytes == 0
            || self.maximum_wire_bytes > 512 * 1024 * 1024
            || self.maximum_decoded_bytes == 0
            || self.maximum_decoded_bytes > 512 * 1024 * 1024
            || self.wall_ms == 0
            || self.wall_ms > 300000
            || self.robots_max_age_seconds == 0
            || self.robots_max_age_seconds > 86400
        {
            return Err(Error::new(
                "network-limit",
                "Invalid acquisition budget/freshness bounds",
            ));
        }
        Ok(())
    }
}
pub struct AcquiredResource {
    pub requested_url: String,
    pub responses: Vec<ReplayResponse>,
}
impl AcquiredResource {
    pub fn response(&self) -> &ReplayResponse {
        self.responses.last().expect("Acquired chain is nonempty")
    }
    pub fn receipt(&self) -> Value {
        json!({"schema":"ddi-danoni-http-resource/1","requestedUrl":self.requested_url,"responses":self.responses.iter().map(ReplayResponse::receipt).collect::<Vec<_>>()})
    }
}
/// One bounded operation. The caller supplies the site-terms/access decision for
/// each new URL; no work script controls cache roots, DNS settings or policies.
pub struct Fetcher {
    cache: CacheStore,
    options: FetchOptions,
    transport: Option<HttpTransport>,
    start: Instant,
    counts: TransferCounts,
    pacer: Pacer,
    policies: BTreeMap<String, Result<Vec<ReplayResponse>>>,
}
impl crate::assets::AssetSource for Fetcher {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        self.read_shared(ns, path, max).map(|b| b.as_ref().clone())
    }
    fn read_shared(&self, ns: &str, path: &str, max: usize) -> Result<std::sync::Arc<Vec<u8>>> {
        self.cache.read_shared(ns, path, max)
    }
}
impl Fetcher {
    pub fn open(cache: &Path, options: FetchOptions) -> Result<Self> {
        options.validate()?;
        Ok(Self {
            cache: CacheStore::open(cache)?,
            options,
            transport: None,
            start: Instant::now(),
            counts: TransferCounts::default(),
            pacer: Pacer::default(),
            policies: BTreeMap::new(),
        })
    }
    pub fn cache_root(&self) -> &Path {
        &self.cache.root
    }
    pub fn counts(&self) -> &TransferCounts {
        &self.counts
    }
    fn deadline(&self) -> Instant {
        self.start + Duration::from_millis(self.options.wall_ms)
    }
    /// Follow at most eight explicit redirects, checking policy per actual hop.
    /// Returns transport evidence, not a playable/normalized-work Ready marker.
    pub async fn fetch(
        &mut self,
        request: &ReplayRequest,
        cancel: &NetworkCancel,
        site: &mut impl FnMut(&str) -> Result<String>,
    ) -> Result<AcquiredResource> {
        self.fetch_checked(request, cancel, site, &mut |_| Ok(()))
            .await
    }
    /// Apply a host identity constraint before every resource hop, including
    /// cached redirects. It cannot grant access or bypass the site/robots policy.
    pub async fn fetch_checked(
        &mut self,
        request: &ReplayRequest,
        cancel: &NetworkCancel,
        site: &mut impl FnMut(&str) -> Result<String>,
        hop: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<AcquiredResource> {
        request.context.validate()?;
        if request.maximum_bytes == 0 || request.maximum_bytes > MEDIA_LIMIT {
            return Err(Error::new("network-limit", "Invalid response byte limit"));
        }
        let requested = address(&request.url)?.to_string();
        let mut next = requested.clone();
        let mut responses = vec![];
        let mut seen = BTreeSet::new();
        loop {
            cancel.check(self.deadline())?;
            if !seen.insert(next.clone()) {
                return Err(Error::new("redirect-loop", "Acquisition redirect loop"));
            }
            hop(&next)?;
            let req = ReplayRequest {
                url: next,
                ..request.clone()
            };
            let response = self.resource(&req, cancel, site).await?;
            let redirect = response.redirect_to.clone();
            responses.push(response);
            if let Some(url) = redirect {
                if responses.len() > 8 {
                    return Err(Error::new("redirect-limit", "Acquisition redirect limit"));
                }
                next = url;
            } else {
                break;
            }
        }
        cancel.check(self.deadline())?;
        Ok(AcquiredResource {
            requested_url: requested,
            responses,
        })
    }
    async fn resource(
        &mut self,
        request: &ReplayRequest,
        cancel: &NetworkCancel,
        site: &mut impl FnMut(&str) -> Result<String>,
    ) -> Result<ReplayResponse> {
        if let Some(r) = self.cached(request)? {
            return Ok(r);
        }
        if request.context.kind != ContextKind::Anonymous {
            return Err(Error::new(
                "request-context-denied",
                "Live acquisition currently admits anonymous GETs only",
            ));
        }
        let url = address(&request.url)?.to_string();
        let site_reason = self.site(&url, site)?;
        let policy = self.policy(&url, cancel, site).await?;
        if !policy.allowed {
            let error = Error::new(
                "robots-denied",
                format!(
                    "{} disallows {url} (body {})",
                    policy.robots_url, policy.body.sha256
                ),
            );
            self.cache.refused(&url, &error)?;
            return Err(error);
        }
        self.live(request, "resource", Some(policy), site_reason, cancel)
            .await
    }
    fn cached(&self, request: &ReplayRequest) -> Result<Option<ReplayResponse>> {
        match self.cache.lookup(request, false)? {
            ReplayLookup::Hit(r) => Ok(Some(*r)),
            ReplayLookup::RecordedFailure { record, .. } => Err(Error::new(
                "cached-request-failure",
                format!(
                    "{}: {}",
                    record["code"].as_str().unwrap_or("unknown"),
                    record["message"].as_str().unwrap_or("Recorded refusal")
                ),
            )),
            ReplayLookup::Miss { canonical_url, .. } => {
                if canonical_url != address(&request.url)?.as_str() {
                    return Err(Error::new(
                        "offline-miss",
                        "Reviewed alias has no canonical response; no inferred fetch",
                    ));
                }
                if self.options.offline {
                    return Err(Error::new(
                        "offline-miss",
                        format!("No cached response: {}", request.url),
                    ));
                }
                Ok(None)
            }
        }
    }
    fn site(&mut self, url: &str, site: &mut impl FnMut(&str) -> Result<String>) -> Result<String> {
        let result = site(url).and_then(|s| {
            if s.trim().is_empty() || s.len() > 4096 || s.chars().any(char::is_control) {
                Err(Error::new(
                    "site-policy-unavailable",
                    "Missing bounded site access/terms decision",
                ))
            } else {
                Ok(s)
            }
        });
        if let Err(e) = &result
            && e.code == "site-terms-denied"
        {
            self.cache.refused(url, e)?;
        }
        result
    }
    async fn policy(
        &mut self,
        target: &str,
        cancel: &NetworkCancel,
        site: &mut impl FnMut(&str) -> Result<String>,
    ) -> Result<RobotsDecision> {
        let origin = address(target)?.origin().ascii_serialization();
        if !self.policies.contains_key(&origin) {
            if self.policies.len() >= 64 {
                return Err(Error::new("network-limit", "Robots origin budget"));
            }
            let chain = self.policy_chain(&origin, cancel, site).await;
            self.policies.insert(origin.clone(), chain);
        }
        let chain = self
            .policies
            .get(&origin)
            .unwrap()
            .as_ref()
            .map_err(Clone::clone)?;
        for r in chain {
            let date = r.selected_response["retrievedAt"]
                .as_str()
                .ok_or_else(|| Error::new("robots-stale", "Missing policy retrieval date"))?;
            let date = time::OffsetDateTime::parse(
                date,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| Error::new("robots-stale", "Policy retrieval date is not RFC3339"))?;
            let age = (time::OffsetDateTime::now_utc() - date).whole_seconds();
            if age < -300 || age > self.options.robots_max_age_seconds as i64 {
                return Err(Error::new(
                    "robots-stale",
                    "Cached policy is stale or in the future; use a fresh acquisition cache generation",
                ));
            }
        }
        evaluate_robots_chain(chain, target)
    }
    async fn policy_chain(
        &mut self,
        origin: &str,
        cancel: &NetworkCancel,
        site: &mut impl FnMut(&str) -> Result<String>,
    ) -> Result<Vec<ReplayResponse>> {
        let mut next = format!("{origin}/robots.txt");
        let mut seen = BTreeSet::new();
        let mut responses = vec![];
        loop {
            cancel.check(self.deadline())?;
            if !seen.insert(next.clone()) {
                return Err(Error::new("robots-unavailable", "Robots redirect loop"));
            }
            let req = ReplayRequest {
                url: next,
                context: RequestContext::default(),
                maximum_bytes: ROBOTS_LIMIT,
            };
            let r = if let Some(r) = self.cached(&req)? {
                r
            } else {
                let reason = self.site(&req.url, site)?;
                self.live(&req, "robots", None, reason, cancel).await?
            };
            let redirect = r.redirect_to.clone();
            responses.push(r);
            if let Some(url) = redirect {
                if responses.len() > 8 {
                    return Err(Error::new("robots-unavailable", "Robots redirect limit"));
                }
                next = url;
            } else {
                return Ok(responses);
            }
        }
    }
    async fn live(
        &mut self,
        request: &ReplayRequest,
        role: &str,
        policy: Option<RobotsDecision>,
        site_reason: String,
        cancel: &NetworkCancel,
    ) -> Result<ReplayResponse> {
        let deadline = self
            .deadline()
            .min(Instant::now() + Duration::from_millis(self.options.transport.request_timeout_ms));
        cancel.check(deadline)?;
        if request.context.kind != ContextKind::Anonymous {
            return Err(Error::new(
                "request-context-denied",
                "Live acquisition currently admits anonymous GETs only",
            ));
        }
        if self.counts.requests >= self.options.maximum_requests {
            return Err(Error::new("network-limit", "New-request budget exhausted"));
        }
        let wire = self
            .options
            .maximum_wire_bytes
            .saturating_sub(self.counts.wire_bytes)
            .min(MEDIA_LIMIT as u64) as usize;
        let decoded = self
            .options
            .maximum_decoded_bytes
            .saturating_sub(self.counts.decoded_bytes)
            .min(request.maximum_bytes as u64) as usize;
        if wire == 0 || decoded == 0 {
            return Err(Error::new(
                "network-byte-limit",
                "Operation transfer budget exhausted",
            ));
        }
        if self.transport.is_none() {
            self.transport = Some(HttpTransport::new(self.options.transport.clone())?);
        }
        let transport = self.transport.as_ref().unwrap();
        let prepared = transport.prepare(&request.url, cancel, deadline).await?;
        let elapsed = self.start.elapsed().as_millis() as u64;
        let delay = policy.as_ref().and_then(|p| p.crawl_delay_ms);
        let ready = self.pacer.ready_at(&request.url, elapsed, delay)?;
        let pause = Duration::from_millis(ready.saturating_sub(elapsed)).max(
            self.cache
                .cooldown(&request.url, delay.unwrap_or(0).max(1000))?,
        );
        cancel.sleep(deadline, pause).await?;
        cancel.check(deadline)?;
        let id = self.cache.begin(&request.url)?;
        if let Err(e) = cancel.check(deadline) {
            self.cache.unstarted(&request.url, &id)?;
            return Err(e);
        }
        if let Err(e) =
            self.pacer
                .mark_started(&request.url, self.start.elapsed().as_millis() as u64, delay)
        {
            self.cache.unstarted(&request.url, &id)?;
            return Err(e);
        }
        let bounded = ReplayRequest {
            maximum_bytes: decoded,
            ..request.clone()
        };
        let result = transport
            .send(prepared, &bounded, wire, cancel, deadline, &mut self.counts)
            .await;
        let response = match result {
            Ok(r) => r,
            Err(e) => {
                self.cache.failure(&request.url, &id, &e)?;
                return Err(e);
            }
        };
        let hash = sha256(&response.body);
        let content_type = response
            .headers
            .get("content-type")
            .map(String::as_str)
            .unwrap_or("application/octet-stream");
        let charset = content_type.split(';').skip(1).find_map(|s| {
            s.trim()
                .split_once('=')
                .filter(|(k, _)| k.trim().eq_ignore_ascii_case("charset"))
                .map(|(_, v)| v.trim().trim_matches(['\'', '"']).to_string())
        });
        let robots = policy
            .as_ref()
            .map(|p| {
                json!([{
                    "url": p.robots_url, "sha256": p.body.sha256,
                    "status": p.status, "userAgent": USER_AGENT, "decision": p
                }])
            })
            .unwrap_or_else(|| json!([]));
        let record = json!({
            "requestedUrl": request.url, "url": request.url, "finalUrl": request.url,
            "status": response.status, "headers": response.headers,
            "mimeType": content_type.split(';').next().unwrap().trim().to_ascii_lowercase(),
            "charset": charset, "sha256": hash, "bodyPath": format!("blobs/sha256/{hash}"),
            "byteLength": response.body.len(), "completeBody": true, "retrievedAt": stamp()?,
            "redirects": [], "robots": robots, "role": role, "fromCache": false,
            "requestContext": request.context,
            "requestContextFingerprint": request.context.fingerprint()?,
            "network": response.network, "sitePolicy": site_reason
        });
        self.cache
            .success(&request.url, &id, record, response.body)?;
        cancel.check(self.deadline())?;
        match self.cache.lookup(request, true)? {
            ReplayLookup::Hit(r) => Ok(*r),
            _ => Err(rejected("Published response did not replay")),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema: String,
    cache: PathBuf,
    #[serde(default)]
    options: FetchOptions,
    request: ReplayRequest,
    origins: BTreeMap<String, String>,
}
pub fn fetch_job(path: &Path) -> Result<Value> {
    use std::io::Read;
    let mut b = vec![];
    std::fs::File::open(path)
        .map_err(crate::native::io)?
        .take(crate::assets::JSON_LIMIT as u64 + 1)
        .read_to_end(&mut b)
        .map_err(crate::native::io)?;
    if b.len() > crate::assets::JSON_LIMIT {
        return Err(Error::new("network-limit", "Job JSON bound"));
    }
    let job: Job = serde_json::from_slice(&b).map_err(|e| rejected(e.to_string()))?;
    if job.schema != "ddi-danoni-http-job/1" {
        return Err(rejected("Unknown HTTP job schema"));
    }
    for (origin, reason) in &job.origins {
        if address(origin)?.origin().ascii_serialization() != *origin || reason.trim().is_empty() {
            return Err(Error::new(
                "site-policy-unavailable",
                "Expected reviewed serialized origins and reasons",
            ));
        }
    }
    let root = if job.cache.is_absolute() {
        job.cache
    } else {
        path.parent().unwrap_or(Path::new(".")).join(job.cache)
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::native::io)?;
    runtime.block_on(async {
        let mut fetcher = Fetcher::open(&root, job.options)?;
        let cancel = NetworkCancel::default();
        let acquired = fetcher
            .fetch(&job.request, &cancel, &mut |url| {
                job.origins
                    .get(&address(url)?.origin().ascii_serialization())
                    .cloned()
                    .ok_or_else(|| {
                        Error::new(
                            "site-terms-denied",
                            "Origin has no host-reviewed public-browser-work decision",
                        )
                    })
            })
            .await?;
        Ok(json!({
            "schema": "ddi-danoni-http-acquisition/1", "cache": fetcher.cache_root(),
            "resource": acquired.receipt(), "transfers": fetcher.counts()
        }))
    })
}

#[cfg(test)]
#[path = "tests_http.rs"]
pub(crate) mod tests;
