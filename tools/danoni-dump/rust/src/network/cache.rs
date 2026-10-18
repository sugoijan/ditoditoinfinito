use super::*;
use crate::{assets::*, native::DirectorySource};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
const ENTRIES_LIMIT: usize = 16384;
const ALIAS_LIMIT: usize = 4096;
const CHAIN_LIMIT: usize = 64;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplayRequest {
    pub url: String,
    #[serde(default)]
    pub context: RequestContext,
    #[serde(default = "maximum")]
    pub maximum_bytes: usize,
}
fn maximum() -> usize {
    MEDIA_LIMIT
}
#[derive(Deserialize)]
struct Index {
    schema: String,
    entries: BTreeMap<String, Value>,
    #[serde(default)]
    aliases: Vec<(String, String)>,
    #[serde(default)]
    failures: BTreeMap<String, Value>,
}
/// One owned replay session, with immutable bound index and read-once bodies.
/// Only a Miss is a missing cache entry; errors/refusals are never retry hints.
pub struct CacheReplay<'a, S: AssetSource> {
    index: Binding,
    entries: BTreeMap<String, Value>,
    aliases: BTreeMap<String, String>,
    failures: BTreeMap<String, Value>,
    inputs: Inputs<'a, S>,
}
#[derive(Clone)]
pub struct ReplayResponse {
    pub request: ReplayRequest,
    pub canonical_url: String,
    pub response_url: String,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub redirect_to: Option<String>,
    pub index: Binding,
    pub body_binding: Binding,
    pub body: Arc<Vec<u8>>,
    /// Unchanged collapsed cache record; the selected hop is identified separately.
    pub resource: Value,
    pub selected_response: Value,
}
impl ReplayResponse {
    /// Passive evidence with no copied body and no live authorization assertion.
    pub fn receipt(&self) -> Value {
        json!({"kind":"hit","request":self.request,"canonicalUrl":self.canonical_url,
            "responseUrl":self.response_url,"status":self.status,"headers":self.headers,
            "redirectTo":self.redirect_to,"index":self.index,"body":self.body_binding,
            "byteLength":self.body.len(),"resource":self.resource,"selectedResponse":self.selected_response})
    }
}
pub enum ReplayLookup {
    Hit(Box<ReplayResponse>),
    Miss {
        requested_url: String,
        canonical_url: String,
    },
    RecordedFailure {
        requested_url: String,
        canonical_url: String,
        index: Binding,
        record: Value,
    },
}
impl ReplayLookup {
    pub fn receipt(&self) -> Value {
        match self {
            Self::Hit(r) => r.receipt(),
            Self::Miss {
                requested_url,
                canonical_url,
            } => json!({"kind":"miss","requestedUrl":requested_url,"canonicalUrl":canonical_url}),
            Self::RecordedFailure {
                requested_url,
                canonical_url,
                index,
                record,
            } => {
                json!({"kind":"recorded-failure","requestedUrl":requested_url,"canonicalUrl":canonical_url,"index":index,"record":record})
            }
        }
    }
}
fn canonical(raw: &str) -> Result<String> {
    let normalized = address(raw)?.to_string();
    if normalized != raw {
        return Err(rejected("Cache URL keys must already be canonical"));
    }
    Ok(normalized)
}
fn redirect(status: u16) -> bool {
    [301, 302, 303, 307, 308].contains(&status)
}
impl<'a, S: AssetSource> CacheReplay<'a, S> {
    pub fn open(source: &'a S, index: Binding) -> Result<Self> {
        let mut inputs = Inputs::new(source);
        let bytes = inputs.checked(&index, JSON_LIMIT)?;
        let parsed: Index = serde_json::from_slice(&bytes).map_err(|e| rejected(e.to_string()))?;
        if parsed.schema != "ddi-danoni-cache/1" {
            return Err(rejected("Unsupported cache index version"));
        }
        if parsed.entries.len() > ENTRIES_LIMIT
            || parsed.failures.len() > ENTRIES_LIMIT
            || parsed.aliases.len() > ALIAS_LIMIT
        {
            return Err(Error::new("cache-limit", "Cache entry/alias limit"));
        }
        let mut aliases = BTreeMap::new();
        for (alias, target) in parsed.aliases {
            canonical(&alias)?;
            canonical(&target)?;
            if address(&alias)?.origin() != address(&target)?.origin()
                || aliases.insert(alias, target).is_some()
            {
                return Err(rejected("Duplicate or cross-origin alias"));
            }
        }
        for (key, record) in &parsed.entries {
            canonical(key)?;
            if record["requestedUrl"] != *key {
                return Err(rejected("Cache record/key identity differs"));
            }
            if parsed.failures.contains_key(key) {
                return Err(rejected("Response conflicts with recorded failure"));
            }
        }
        for (key, record) in &parsed.failures {
            canonical(key)?;
            if record["code"]
                .as_str()
                .is_none_or(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
                || record["message"].as_str().is_none_or(|s| s.len() > 4096)
            {
                return Err(rejected("Invalid recorded failure"));
            }
        }
        let result = Self {
            index,
            entries: parsed.entries,
            aliases,
            failures: parsed.failures,
            inputs,
        };
        for alias in result.aliases.keys() {
            result.resolve(alias)?;
        }
        Ok(result)
    }
    /// Follow only existing, hash-bound robots responses. Every redirect remains
    /// a separate checked response; no lookup failure or miss triggers a fetch.
    pub fn robots(&mut self, target: &str) -> Result<super::RobotsDecision> {
        let origin = address(target)?.origin().ascii_serialization();
        let mut next = format!("{origin}/robots.txt");
        let mut seen = std::collections::BTreeSet::new();
        let mut responses = Vec::new();
        loop {
            if !seen.insert(next.clone()) {
                return Err(Error::new("robots-unavailable", "Robots redirect loop"));
            }
            let request = ReplayRequest {
                url: next,
                context: RequestContext::default(),
                maximum_bytes: super::ROBOTS_LIMIT,
            };
            let response = match self.lookup(&request)? {
                ReplayLookup::Hit(response) => *response,
                _ => {
                    return Err(Error::new(
                        "robots-unavailable",
                        "Exact cached robots response unavailable; no fetch",
                    ));
                }
            };
            let redirect = response.redirect_to.clone();
            responses.push(response);
            if let Some(target) = redirect {
                if responses.len() > 8 {
                    return Err(Error::new("robots-unavailable", "Robots redirect limit"));
                }
                next = target;
            } else {
                return super::evaluate_robots_chain(&responses, target);
            }
        }
    }
    pub fn entry_urls(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
    pub fn failure_urls(&self) -> impl Iterator<Item = &str> {
        self.failures.keys().map(String::as_str)
    }
    fn resolve(&self, url: &str) -> Result<String> {
        let mut next = url;
        let mut seen = std::collections::BTreeSet::new();
        while let Some(target) = self.aliases.get(next) {
            if seen.len() >= CHAIN_LIMIT || !seen.insert(next) {
                return Err(rejected("Cyclic or overlong alias chain"));
            }
            next = target;
        }
        Ok(next.into())
    }
    /// Replay one actual response hop. A collapsed redirect record never hides
    /// the original redirect or permits skipping the target's Vary/policy checks.
    pub fn lookup(&mut self, request: &ReplayRequest) -> Result<ReplayLookup> {
        self.lookup_inner(request, false)
    }
    pub(crate) fn lookup_fresh(&mut self, request: &ReplayRequest) -> Result<ReplayLookup> {
        self.lookup_inner(request, true)
    }
    fn lookup_inner(&mut self, request: &ReplayRequest, fresh: bool) -> Result<ReplayLookup> {
        request.context.validate()?;
        if request.maximum_bytes == 0 || request.maximum_bytes > MEDIA_LIMIT {
            return Err(Error::new("cache-limit", "Invalid replay body limit"));
        }
        let requested = address(&request.url)?.to_string();
        let canonical = self.resolve(&requested)?;
        if let Some(record) = self
            .failures
            .get(&requested)
            .or_else(|| self.failures.get(&canonical))
        {
            return Ok(ReplayLookup::RecordedFailure {
                requested_url: requested,
                canonical_url: canonical,
                index: self.index.clone(),
                record: record.clone(),
            });
        }
        let Some(record) = self.entries.get(&canonical) else {
            return Ok(ReplayLookup::Miss {
                requested_url: requested,
                canonical_url: canonical,
            });
        };
        let hops = record["redirects"]
            .as_array()
            .ok_or_else(|| rejected("Missing redirect inventory"))?;
        if hops.len() > 8 {
            return Err(Error::new(
                "redirect-limit",
                "More than eight recorded redirect hops",
            ));
        }
        let selected = if let Some(first) = hops.first() {
            if first["url"] != canonical {
                return Err(rejected("First redirect identity differs"));
            }
            first
        } else {
            record
        };
        let status = selected["status"]
            .as_u64()
            .filter(|s| (100..=599).contains(s))
            .ok_or_else(|| rejected("Invalid HTTP status"))? as u16;
        if status == 206 || selected["completeBody"] == false {
            return Err(Error::new(
                "partial-response",
                "Cached response is incomplete; no refetch",
            ));
        }
        if hops.is_empty() && record["completeBody"] != true {
            return Err(rejected("Missing final response completeness"));
        }
        if !hops.is_empty() && !redirect(status) {
            return Err(rejected("Recorded hop is not a redirect"));
        }
        let response_url = address(if hops.is_empty() {
            record["finalUrl"]
                .as_str()
                .ok_or_else(|| rejected("Missing final URL"))?
        } else {
            &canonical
        })?;
        if hops.is_empty() && response_url.as_str() != canonical {
            // Reviewed timestamp aliases may retain the exact original retrieval
            // URL; a cache migration/alias is explicit evidence, not a guess.
            let migrated = record["cacheMigration"]["sourceUrl"]
                .as_str()
                .or_else(|| record["aliasOf"].as_str());
            if migrated != Some(response_url.as_str())
                || address(&canonical)?.origin() != response_url.origin()
            {
                return Err(rejected(
                    "Changed final URL has no recorded redirect or alias evidence",
                ));
            }
        }
        let headers: BTreeMap<String, String> = serde_json::from_value(selected["headers"].clone())
            .map_err(|e| rejected(e.to_string()))?;
        if headers.len() > 128
            || headers.iter().any(|(k, v)| {
                k.is_empty()
                    || k.len() > 128
                    || k != &k.to_ascii_lowercase()
                    || !k
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-!#$%&'*+.^_`|~".contains(&b))
                    || v.len() > 16384
                    || k.chars().any(char::is_control)
                    || v.chars().any(char::is_control)
            })
        {
            return Err(rejected("Invalid cached headers"));
        }
        if fresh {
            if context(selected)? != request.context {
                return Err(rejected("Fresh response context differs"));
            }
        } else {
            variant(selected, &request.context, &address(&canonical)?)?;
        }
        let redirect_to = if redirect(status) {
            let target = headers
                .get("location")
                .ok_or_else(|| rejected("Missing redirect Location"))?;
            let target = address(
                response_url
                    .join(target)
                    .map_err(|e| rejected(e.to_string()))?
                    .as_str(),
            )?;
            if target.as_str() == response_url.as_str() {
                return Err(Error::new("redirect-loop", "Self redirect"));
            }
            if !hops.is_empty() && selected["location"] != target.as_str() {
                return Err(rejected("Stored redirect target differs from Location"));
            }
            Some(target.into())
        } else {
            None
        };
        let hash = selected["sha256"]
            .as_str()
            .ok_or_else(|| rejected("Missing body digest"))?;
        let expected_path = format!("blobs/sha256/{hash}");
        if !valid_hash(hash) || selected["bodyPath"] != expected_path {
            return Err(rejected(
                "Body must be the exact content-addressed cache path",
            ));
        }
        let length = selected["byteLength"]
            .as_u64()
            .ok_or_else(|| rejected("Missing body byte length"))?;
        if length > request.maximum_bytes as u64 {
            return Err(Error::new(
                "cache-body-limit",
                "Cached body exceeds this request's bound",
            ));
        }
        if selected["retrievedAt"]
            .as_str()
            .is_none_or(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        {
            return Err(rejected("Missing bounded retrieval stamp"));
        }
        let body_binding = self.index.sibling(&RelativeBinding {
            path: expected_path,
            sha256: hash.into(),
        })?;
        let body = self.inputs.checked(&body_binding, request.maximum_bytes)?;
        if body.len() as u64 != length {
            return Err(rejected("Cached body length differs"));
        }
        Ok(ReplayLookup::Hit(Box::new(ReplayResponse {
            request: ReplayRequest {
                url: requested,
                ..request.clone()
            },
            canonical_url: canonical,
            response_url: response_url.into(),
            status,
            headers,
            redirect_to,
            index: self.index.clone(),
            body_binding,
            body,
            resource: record.clone(),
            selected_response: selected.clone(),
        })))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    schema: String,
    roots: BTreeMap<String, PathBuf>,
    index: Binding,
    request: ReplayRequest,
}
pub fn replay_job(path: &Path) -> Result<Value> {
    use std::io::Read;
    let mut bytes = vec![];
    std::fs::File::open(path)
        .map_err(crate::native::io)?
        .take(JSON_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(crate::native::io)?;
    if bytes.len() > JSON_LIMIT {
        return Err(Error::new("cache-limit", "Job JSON limit"));
    }
    let job: Job = serde_json::from_slice(&bytes).map_err(|e| rejected(e.to_string()))?;
    if job.schema != "ddi-danoni-cache-replay-job/1" {
        return Err(rejected("Unknown cache replay job version"));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let roots = job
        .roots
        .into_iter()
        .map(|(ns, p)| (ns, if p.is_absolute() { p } else { parent.join(p) }))
        .collect();
    let source = DirectorySource::new(roots)?;
    let mut cache = CacheReplay::open(&source, job.index)?;
    Ok(
        json!({"schema":"ddi-danoni-cache-replay/1","result":cache.lookup(&job.request)?.receipt(),"networkRequests":0}),
    )
}
