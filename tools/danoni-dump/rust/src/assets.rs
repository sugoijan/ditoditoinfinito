//! Host-supplied bytes and contained names; the core never opens a file.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

pub const JSON_LIMIT: usize = 16 * 1024 * 1024;
pub const EVIDENCE_LIMIT: usize = 8 * 1024 * 1024;
pub const MEDIA_LIMIT: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub namespace: String,
    pub path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelativeBinding {
    pub path: String,
    pub sha256: String,
}
impl Binding {
    pub fn sibling(&self, row: &RelativeBinding) -> Result<Self> {
        relative_path(&row.path)?;
        let prefix = self.path.rsplit_once('/').map_or("", |(p, _)| p);
        Ok(Self {
            namespace: self.namespace.clone(),
            path: if prefix.is_empty() {
                row.path.clone()
            } else {
                format!("{prefix}/{}", row.path)
            },
            sha256: row.sha256.clone(),
        })
    }
}
pub fn relative_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 4096
        || path
            .chars()
            .any(|c| c == '\\' || c == ':' || c.is_control())
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(Error::new(
            "unsafe-path",
            "Expected a contained, portable relative file path",
        ));
    }
    Ok(())
}
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub(crate) fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Implementations must bound reads and confine each namespace to its own root.
pub trait AssetSource {
    fn read(&self, namespace: &str, path: &str, maximum_bytes: usize) -> Result<Vec<u8>>;
}
#[derive(Clone, Debug)]
pub struct Artifact {
    pub path: String,
    pub bytes: Arc<Vec<u8>>,
}

pub(crate) struct Inputs<'a, S: AssetSource> {
    pub source: &'a S,
    cache: BTreeMap<(String, String), Arc<Vec<u8>>>,
    pub used: BTreeMap<String, Binding>,
    total: usize,
}
impl<'a, S: AssetSource> Inputs<'a, S> {
    pub fn new(source: &'a S) -> Self {
        Self {
            source,
            cache: BTreeMap::new(),
            used: BTreeMap::new(),
            total: 0,
        }
    }
    pub fn checked(&mut self, binding: &Binding, maximum: usize) -> Result<Arc<Vec<u8>>> {
        relative_path(&binding.path)?;
        if binding.namespace.is_empty() || !valid_hash(&binding.sha256) {
            return Err(Error::new(
                "invalid-binding",
                "Need a namespace and lowercase SHA-256",
            ));
        }
        let key = (binding.namespace.clone(), binding.path.clone());
        let body = if let Some(body) = self.cache.get(&key) {
            body.clone()
        } else {
            let bytes = self
                .source
                .read(&binding.namespace, &binding.path, maximum)?;
            if bytes.len() > maximum
                || self
                    .total
                    .checked_add(bytes.len())
                    .is_none_or(|n| n > 512 * 1024 * 1024)
            {
                return Err(Error::new(
                    "asset-limit",
                    "Input exceeds its file or 512 MiB work limit",
                ));
            }
            self.total += bytes.len();
            let bytes = Arc::new(bytes);
            self.cache.insert(key, bytes.clone());
            bytes
        };
        if body.len() > maximum || sha256(&body) != binding.sha256 {
            return Err(Error::new(
                "changed-asset",
                format!(
                    "Asset size/hash mismatch: {}:{}",
                    binding.namespace, binding.path
                ),
            ));
        }
        self.used.insert(
            format!("{}:{}", binding.namespace, binding.path),
            binding.clone(),
        );
        Ok(body)
    }
    pub fn local(
        &mut self,
        manifest: &Binding,
        path: &str,
        hash: &str,
        maximum: usize,
    ) -> Result<Arc<Vec<u8>>> {
        self.checked(
            &manifest.sibling(&RelativeBinding {
                path: path.into(),
                sha256: hash.into(),
            })?,
            maximum,
        )
    }
    pub fn reference(&mut self, manifest: &Binding, path: &str) -> Result<Arc<Vec<u8>>> {
        // The exact manifest binds this path; preserve/hash its supplied bytes in
        // the result even when older source schemas did not provide its digest.
        let binding = manifest.sibling(&RelativeBinding {
            path: path.into(),
            sha256: String::new(),
        })?;
        let key = (binding.namespace.clone(), binding.path.clone());
        if let Some(body) = self.cache.get(&key) {
            return Ok(body.clone());
        }
        let body = self
            .source
            .read(&binding.namespace, &binding.path, JSON_LIMIT)?;
        let binding = Binding {
            sha256: sha256(&body),
            ..binding
        };
        // Populate without reading the reference a second time.
        let key = (binding.namespace.clone(), binding.path.clone());
        if body.len() > JSON_LIMIT
            || self
                .total
                .checked_add(body.len())
                .is_none_or(|n| n > 512 * 1024 * 1024)
        {
            return Err(Error::new("asset-limit", "Oversized reference"));
        }
        self.total += body.len();
        self.cache.insert(key, Arc::new(body));
        self.checked(&binding, JSON_LIMIT)
    }
}
pub(crate) fn json<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|e| Error::new("invalid-json", e.to_string()))
}
pub(crate) fn text<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| Error::new("invalid-document", format!("Expected text field {key}")))
}
