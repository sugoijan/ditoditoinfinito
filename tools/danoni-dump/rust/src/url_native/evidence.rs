use super::*;
use std::sync::{Arc, Mutex};
type SharedFiles = Arc<Mutex<BTreeMap<String, Arc<Vec<u8>>>>>;
#[derive(Clone, Default)]
pub(super) struct Evidence(SharedFiles);
impl AssetSource for Evidence {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        self.read_shared(ns, path, max).map(|b| b.as_ref().clone())
    }
    fn read_shared(&self, ns: &str, path: &str, max: usize) -> Result<Arc<Vec<u8>>> {
        relative_path(path)?;
        if ns != "url" {
            return Err(invalid("Unknown URL input namespace"));
        }
        self.0
            .lock()
            .map_err(|_| invalid("Evidence lock poisoned"))?
            .get(path)
            .filter(|b| b.len() <= max)
            .cloned()
            .ok_or_else(|| invalid("Missing or oversized URL evidence"))
    }
}
impl Evidence {
    pub fn add(&self, root: &Path, path: String, body: Arc<Vec<u8>>) -> Result<Binding> {
        relative_path(&path)?;
        let mut files = self
            .0
            .lock()
            .map_err(|_| invalid("Evidence lock poisoned"))?;
        if let Some(old) = files.get(&path) {
            if **old != *body {
                return Err(invalid("Conflicting immutable evidence"));
            }
        } else {
            if files.len() >= 4096
                || files
                    .values()
                    .map(|b| b.len())
                    .sum::<usize>()
                    .checked_add(body.len())
                    .is_none_or(|n| n > 512 * 1024 * 1024)
            {
                return Err(invalid("URL evidence exceeds 4096 files / 512 MiB"));
            }
            orchestration::write(root, &path, &body)?;
            files.insert(path.clone(), body.clone());
        }
        Ok(Binding {
            namespace: "url".into(),
            path,
            sha256: sha256(&body),
        })
    }
    pub fn json(&self, root: &Path, path: String, value: &impl Serialize) -> Result<Binding> {
        let body = serde_json::to_vec_pretty(value).map_err(|e| invalid(e.to_string()))?;
        if body.len() > JSON_LIMIT {
            return Err(invalid("URL evidence JSON limit"));
        }
        self.add(root, path, Arc::new(body))
    }
    pub fn archive(
        &self,
        root: &Path,
        fetcher: &Fetcher,
        resource: &AcquiredResource,
    ) -> Result<Binding> {
        let receipt = resource.receipt();
        let mut bindings = BTreeMap::new();
        collect(&receipt, &mut bindings)?;
        let mut inputs = Inputs::new(fetcher);
        for binding in bindings.values() {
            let b = inputs.checked(binding, MEDIA_LIMIT)?;
            self.add(root, format!("cache/{}", binding.path), b)?;
        }
        self.json(
            root,
            format!("http/{}.json", sha256(resource.requested_url.as_bytes())),
            &receipt,
        )
    }
    pub fn bindings(&self) -> Result<Vec<Binding>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| invalid("Evidence lock poisoned"))?
            .iter()
            .map(|(path, body)| Binding {
                namespace: "url".into(),
                path: path.clone(),
                sha256: sha256(body),
            })
            .collect())
    }
    pub fn body(&self, binding: &Binding) -> Binding {
        Binding {
            namespace: "url".into(),
            path: format!("cache/{}", binding.path),
            sha256: binding.sha256.clone(),
        }
    }
}
// Capture immutable response/robots bindings, including nested policy chains.
// Bound reads validate every digest/path; metadata cannot cause a network request.
fn collect(value: &Value, result: &mut BTreeMap<String, Binding>) -> Result<()> {
    if result.len() > 4096 {
        return Err(invalid("HTTP evidence binding limit"));
    }
    match value {
        Value::Object(map) => {
            if map.get("namespace").and_then(Value::as_str) == Some("cache")
                && map.contains_key("path")
                && map.contains_key("sha256")
            {
                let b: Binding =
                    serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
                relative_path(&b.path)?;
                if let Some(old) = result.insert(b.path.clone(), b.clone())
                    && old.sha256 != b.sha256
                {
                    return Err(invalid("HTTP binding conflict"));
                }
            } else {
                for v in map.values() {
                    collect(v, result)?;
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                collect(v, result)?;
            }
        }
        _ => {}
    }
    Ok(())
}
