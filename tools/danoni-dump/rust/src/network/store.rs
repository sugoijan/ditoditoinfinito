use super::*;
use crate::{
    assets::*,
    native::{self, DirectorySource},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) fn stamp() -> Result<String> {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| Error::new("clock", e.to_string()))
}
fn bytes(v: &Value) -> Result<Vec<u8>> {
    let mut b = serde_json::to_vec(v).map_err(|e| rejected(e.to_string()))?;
    b.push(b'\n');
    if b.len() > JSON_LIMIT {
        return Err(Error::new("cache-limit", "Index/capsule JSON limit"));
    }
    Ok(b)
}
struct SharedFiles {
    directory: DirectorySource,
    files: RefCell<BTreeMap<String, Arc<Vec<u8>>>>,
    total: RefCell<usize>,
}
impl SharedFiles {
    fn insert(&self, path: &str, body: Arc<Vec<u8>>) -> Result<()> {
        if self.files.borrow().contains_key(path) {
            return Ok(());
        }
        let mut total = self.total.borrow_mut();
        if total
            .checked_add(body.len())
            .is_none_or(|n| n > 512 * 1024 * 1024)
        {
            return Err(Error::new("asset-limit", "Cache session memory limit"));
        }
        *total += body.len();
        self.files.borrow_mut().insert(path.into(), body);
        Ok(())
    }
}
impl AssetSource for SharedFiles {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        self.read_shared(ns, path, max).map(|b| b.as_ref().clone())
    }
    fn read_shared(&self, ns: &str, path: &str, max: usize) -> Result<Arc<Vec<u8>>> {
        relative_path(path)?;
        if ns != "cache" {
            return Err(rejected("Unknown cache namespace"));
        }
        if let Some(b) = self.files.borrow().get(path) {
            if b.len() > max {
                return Err(Error::new(
                    "asset-limit",
                    "Cached bytes exceed request bound",
                ));
            }
            return Ok(b.clone());
        }
        let b = Arc::new(self.directory.read(ns, path, max)?);
        self.insert(path, b.clone())?;
        Ok(b)
    }
}
pub(crate) struct CacheStore {
    pub root: PathBuf,
    _lock: File,
    document: Value,
    digest: String,
    files: SharedFiles,
    poisoned: bool,
}
fn safe_file(root: &Path, name: &str) -> Result<()> {
    relative_path(name)?;
    if let Ok(m) = fs::symlink_metadata(root.join(name))
        && (!m.is_file() || m.file_type().is_symlink())
    {
        return Err(rejected("Cache file is not a plain regular file"));
    }
    Ok(())
}
fn directory(root: &Path, path: &str) -> Result<()> {
    relative_path(path)?;
    let mut p = root.to_path_buf();
    for part in path.split('/') {
        p.push(part);
        match fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            Ok(_) => return Err(rejected("Cache directory contains symlink/non-directory")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&p).map_err(native::io)?;
            }
            Err(e) => return Err(native::io(e)),
        }
    }
    Ok(())
}
fn durable(root: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(root)
        .and_then(|f| f.sync_all())
        .map_err(native::io)?;
    Ok(())
}
impl CacheStore {
    pub fn open(path: &Path) -> Result<Self> {
        if !cfg!(unix) {
            return Err(Error::new(
                "unsupported-cache-platform",
                "Durable cache writes currently require Unix directory synchronization",
            ));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map_err(native::io)?;
        native::external(&parent)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| rejected("Cache root name"))?;
        relative_path(name)?;
        let root = parent.join(name);
        if !root.exists() {
            let mut owned = crate::orchestration::new_root(&root)?;
            owned.retain = true;
        }
        let meta = fs::symlink_metadata(&root).map_err(native::io)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(rejected("Cache root must be a directory without symlinks"));
        }
        safe_file(&root, ".writer.lock")?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".writer.lock"))
            .map_err(native::io)?;
        lock.try_lock()
            .map_err(|e| Error::new("cache-locked", e.to_string()))?;
        directory(&root, "blobs/sha256")?;
        let source = DirectorySource::new(BTreeMap::from([("cache".into(), root.clone())]))?;
        let initial = match source.read("cache", "index.json", JSON_LIMIT) {
            Ok(b) => Some(b),
            Err(e) if !root.join("index.json").exists() && e.code == "filesystem" => None,
            Err(e) => return Err(e),
        };
        let mut document: Value = if let Some(b) = &initial {
            let binding = Binding {
                namespace: "cache".into(),
                path: "index.json".into(),
                sha256: sha256(b),
            };
            // The already-read index is supplied as immutable bytes; validate it
            // without touching any work body or reopening the file.
            struct Initial<'a>(&'a [u8]);
            impl AssetSource for Initial<'_> {
                fn read(&self, _: &str, path: &str, max: usize) -> Result<Vec<u8>> {
                    if path != "index.json" || self.0.len() > max {
                        return Err(rejected("Initial index bound"));
                    }
                    Ok(self.0.to_vec())
                }
            }
            let initial = Initial(b);
            CacheReplay::open(&initial, binding)?;
            serde_json::from_slice(b).map_err(|e| rejected(e.to_string()))?
        } else {
            json!({"schema":"ddi-danoni-cache/1","entries":{},"aliases":[],"failures":{}})
        };
        // Both collections were optional in historical cache indexes. Keep
        // replay compatibility while giving the writer its mutable containers.
        if document.get("aliases").is_none() {
            document["aliases"] = json!([]);
        }
        if document.get("failures").is_none() {
            document["failures"] = json!({});
        }
        let digest = initial.as_ref().map(|b| sha256(b)).unwrap_or_default();
        let mut store = Self {
            root,
            _lock: lock,
            document,
            digest,
            files: SharedFiles {
                directory: source,
                files: RefCell::new(BTreeMap::new()),
                total: RefCell::new(0),
            },
            poisoned: false,
        };
        let mut recovered = false;
        for value in store.document["failures"]
            .as_object_mut()
            .unwrap()
            .values_mut()
        {
            if value["code"] == "request-incomplete" {
                value["recoveredAt"] = json!(stamp()?);
                recovered = true;
            }
        }
        if initial.is_none() || recovered {
            store.flush()?;
        }
        Ok(store)
    }
    fn healthy(&self) -> Result<()> {
        if self.poisoned {
            return Err(Error::new(
                "cache-write-failed",
                "Cache store was poisoned by a failed publication",
            ));
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        self.healthy()?;
        let result = (|| {
            let current = if self.root.join("index.json").exists() {
                sha256(
                    &self
                        .files
                        .directory
                        .read("cache", "index.json", JSON_LIMIT)?,
                )
            } else {
                String::new()
            };
            if current != self.digest {
                return Err(rejected("Cache index changed outside the single writer"));
            }
            let b = bytes(&self.document)?;
            let name = format!(
                ".index-{}-{}.tmp",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            let path = self.root.join(&name);
            let mut f = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .map_err(native::io)?;
            f.write_all(&b)
                .and_then(|()| f.sync_all())
                .map_err(native::io)?;
            drop(f);
            fs::rename(&path, self.root.join("index.json")).map_err(native::io)?;
            durable(&self.root)?;
            self.digest = sha256(&b);
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn immutable(&self, path: &str, body: Arc<Vec<u8>>) -> Result<()> {
        relative_path(path)?;
        safe_file(&self.root, path)?;
        let destination = self.root.join(path);
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&destination)
        {
            Ok(mut f) => {
                f.write_all(&body)
                    .and_then(|()| f.sync_all())
                    .map_err(native::io)?;
                drop(f);
                durable(destination.parent().unwrap())?;
                self.files.insert(path, body)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = self.files.read_shared("cache", path, body.len())?;
                if *existing != *body {
                    return Err(rejected("Existing immutable cache file differs"));
                }
            }
            Err(e) => return Err(native::io(e)),
        }
        Ok(())
    }
    pub fn lookup(&self, request: &ReplayRequest, fresh: bool) -> Result<ReplayLookup> {
        self.healthy()?;
        let requested = address(&request.url)?.to_string();
        let mut next = requested.clone();
        let mut aliases = vec![];
        let all = self.document["aliases"]
            .as_array()
            .ok_or_else(|| rejected("Alias inventory"))?;
        while let Some(link) = all.iter().find(|r| r[0] == next) {
            aliases.push(link.clone());
            next = link[1]
                .as_str()
                .ok_or_else(|| rejected("Alias target"))?
                .into();
            if aliases.len() > 64 {
                return Err(rejected("Alias chain bound"));
            }
        }
        let mut entries = serde_json::Map::new();
        let mut failures = serde_json::Map::new();
        if let Some(r) = self.document["entries"].get(&next) {
            entries.insert(next.clone(), r.clone());
        }
        for key in [&requested, &next] {
            if let Some(r) = self.document["failures"].get(key) {
                failures.insert(key.clone(), r.clone());
            }
        }
        if entries.is_empty() && failures.is_empty() {
            return Ok(ReplayLookup::Miss {
                requested_url: requested,
                canonical_url: next,
            });
        }
        let capsule = json!({"schema":"ddi-danoni-cache/1","entries":entries,"aliases":aliases,"failures":failures});
        let b = Arc::new(bytes(&capsule)?);
        let hash = sha256(&b);
        let path = format!("r-{hash}.json");
        self.immutable(&path, b)?;
        let binding = Binding {
            namespace: "cache".into(),
            path,
            sha256: hash,
        };
        let mut replay = CacheReplay::open(&self.files, binding)?;
        if fresh {
            replay.lookup_fresh(request)
        } else {
            replay.lookup(request)
        }
    }
    pub fn cooldown(&self, url: &str, delay_ms: u64) -> Result<std::time::Duration> {
        let origin = address(url)?.origin();
        let now = time::OffsetDateTime::now_utc();
        let mut wait = 0;
        for rows in ["entries", "failures"] {
            for (key, value) in self.document[rows].as_object().unwrap() {
                if address(key)?.origin() != origin {
                    continue;
                }
                let date = value["recoveredAt"]
                    .as_str()
                    .or_else(|| value["retrievedAt"].as_str())
                    .or_else(|| value["failedAt"].as_str());
                if let Some(date) = date {
                    let date = time::OffsetDateTime::parse(
                        date,
                        &time::format_description::well_known::Rfc3339,
                    )
                    .map_err(|_| rejected("Invalid persistent pacing timestamp"))?;
                    let age = (now - date).whole_milliseconds();
                    if age < -300000 {
                        return Err(Error::new(
                            "pacing-clock",
                            "Cache timestamps are in the future",
                        ));
                    }
                    wait = wait.max((delay_ms as i128 - age).max(0) as u64);
                }
            }
        }
        Ok(std::time::Duration::from_millis(wait))
    }
    pub fn begin(&mut self, url: &str) -> Result<String> {
        self.healthy()?;
        if self.document["entries"].get(url).is_some()
            || self.document["failures"].get(url).is_some()
        {
            return Err(rejected("URL already has a response or attempted request"));
        }
        let id = format!(
            "{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        self.document["failures"][url] = json!({"code":"request-incomplete","message":"Durably reserved request; a crash must not trigger a duplicate fetch","failedAt":stamp()?,"attemptId":id});
        self.flush()?;
        Ok(id)
    }
    pub fn unstarted(&mut self, url: &str, id: &str) -> Result<()> {
        self.attempt(url, id)?;
        self.document["failures"]
            .as_object_mut()
            .unwrap()
            .remove(url);
        self.flush()
    }
    fn attempt(&self, url: &str, id: &str) -> Result<()> {
        self.healthy()?;
        if self.document["failures"][url]["attemptId"] != id {
            return Err(rejected("Attempt identity differs"));
        }
        Ok(())
    }
    pub fn failure(&mut self, url: &str, id: &str, error: &Error) -> Result<()> {
        self.attempt(url, id)?;
        self.document["failures"][url] = json!({"code":error.code,"message":error.message.chars().take(2048).collect::<String>(),"failedAt":stamp()?,"attemptId":id});
        self.flush()
    }
    pub fn refused(&mut self, url: &str, error: &Error) -> Result<()> {
        let id = self.begin(url)?;
        self.failure(url, &id, error)
    }
    pub fn success(
        &mut self,
        url: &str,
        id: &str,
        record: Value,
        body: Arc<Vec<u8>>,
    ) -> Result<()> {
        self.attempt(url, id)?;
        if record["requestedUrl"] != url
            || record["sha256"] != sha256(&body)
            || record["byteLength"].as_u64() != Some(body.len() as u64)
        {
            return Err(rejected("New response binding differs"));
        }
        let path = record["bodyPath"]
            .as_str()
            .ok_or_else(|| rejected("Body path"))?;
        if path != format!("blobs/sha256/{}", record["sha256"].as_str().unwrap()) {
            return Err(rejected("Non-canonical new body path"));
        }
        self.immutable(path, body)?;
        self.document["failures"]
            .as_object_mut()
            .unwrap()
            .remove(url);
        self.document["entries"][url] = record;
        self.flush()
    }
}
