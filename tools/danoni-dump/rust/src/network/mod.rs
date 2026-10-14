//! Sans-I/O request policy and historical cache replay. No live transport yet.
mod cache;
mod robots;
mod scheduling;
use crate::{Error, Result, assets::sha256};
pub use cache::{CacheReplay, ReplayLookup, ReplayRequest, ReplayResponse, replay_job};
pub use robots::{ROBOTS_LIMIT, RobotsDecision, evaluate_robots, evaluate_robots_chain};
pub use scheduling::{Pacer, pin_public_addresses};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

pub const USER_AGENT: &str = "DdiDanoniDump";
/// A future transport must send these exact negotiation headers before reusing
/// old cache variants that declare Vary: Accept-Encoding or User-Agent.
pub const ACCEPT_ENCODING: &str = "gzip, deflate, br";
pub(crate) fn address(raw: &str) -> Result<Url> {
    crate::acquisition::http(raw).map_err(|e| Error::new("unsupported-url", e.message))
}
pub(crate) fn rejected(message: impl Into<String>) -> Error {
    Error::new("cache-integrity", message)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextKind {
    Anonymous,
    Browser,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestContext {
    pub kind: ContextKind,
    pub headers: BTreeMap<String, String>,
}
impl Default for RequestContext {
    fn default() -> Self {
        Self {
            kind: ContextKind::Anonymous,
            headers: BTreeMap::from([("accept".into(), "*/*".into())]),
        }
    }
}
impl RequestContext {
    pub fn validate(&self) -> Result<()> {
        if self.kind == ContextKind::Anonymous && self != &Self::default() {
            return Err(Error::new(
                "request-context-denied",
                "Anonymous requests have only Accept: */*",
            ));
        }
        for (key, value) in &self.headers {
            if !["accept", "origin", "referer"].contains(&key.as_str())
                || value.len() > 4096
                || !value.is_ascii()
                || value.chars().any(char::is_control)
            {
                return Err(Error::new(
                    "request-context-denied",
                    "Only bounded actual Accept/Origin/Referer headers are supported",
                ));
            }
            if key == "origin"
                && !value.is_empty()
                && value != "null"
                && address(value)?.origin().ascii_serialization() != *value
            {
                return Err(Error::new(
                    "request-context-denied",
                    "Origin must be a serialized HTTP(S) origin or null",
                ));
            }
            if key == "referer" && !value.is_empty() {
                address(value)?;
            }
        }
        Ok(())
    }
    /// Compatible with CaptureNetwork's JSON.stringify(sorted header entries).
    pub fn fingerprint(&self) -> Result<String> {
        self.validate()?;
        let entries: Vec<_> = self.headers.iter().collect();
        Ok(sha256(
            &serde_json::to_vec(&entries).map_err(|e| rejected(e.to_string()))?,
        ))
    }
}
pub(crate) fn context(record: &serde_json::Value) -> Result<RequestContext> {
    let context: RequestContext = if let Some(c) = record.get("requestContext") {
        serde_json::from_value(c.clone()).map_err(|e| rejected(e.to_string()))?
    } else {
        RequestContext::default()
    };
    let fingerprint = context.fingerprint()?;
    if record.get("requestContextFingerprint").is_some()
        && record["requestContextFingerprint"] != fingerprint
    {
        return Err(rejected("Cached request context fingerprint differs"));
    }
    Ok(context)
}
pub(crate) fn variant(
    record: &serde_json::Value,
    current: &RequestContext,
    url: &Url,
) -> Result<()> {
    let original = context(record)?;
    let vary = record["headers"]["vary"].as_str().unwrap_or("");
    if vary.len() > 4096 {
        return Err(rejected("Oversized Vary"));
    }
    for header in vary
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
    {
        let fixed = match header.as_str() {
            "user-agent" => Some(USER_AGENT),
            "accept-encoding" => Some(ACCEPT_ENCODING),
            "cookie" | "authorization" | "range" => Some(""),
            "host" => Some(url.authority()),
            _ => None,
        };
        if !["accept", "origin", "referer"].contains(&header.as_str()) && fixed.is_none() {
            return Err(Error::new(
                "cache-vary-unsupported",
                format!("Cannot reuse Vary: {header}; no refetch"),
            ));
        }
        let old = original
            .headers
            .get(&header)
            .map(String::as_str)
            .or(fixed)
            .unwrap_or("");
        let new = current
            .headers
            .get(&header)
            .map(String::as_str)
            .or(fixed)
            .unwrap_or("");
        if old != new {
            return Err(Error::new(
                "cache-vary-mismatch",
                format!("Cached Vary: {header} differs; no refetch"),
            ));
        }
    }
    Ok(())
}
