//! Native, exclusive-writer managed library. Publication is one catalog rename.
//!
//! Immutable transaction directories are retained, including aborted attempts.
//! This Unix adapter assumes a private local filesystem and cooperative writers;
//! it is not a sandbox against another process modifying the store concurrently.
use crate::{
    Error, Result,
    assets::{self, JSON_LIMIT, MEDIA_LIMIT, sha256, valid_hash},
    import_host::{self as host, CommitOutcome, Decision, LibraryStore, Transaction},
    native::io,
    prepared_import::{PreparedImport, PreparedSong},
};
use ddi_library::manifest::ManifestEntry;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
const FORMAT: &[u8] = b"ddi-danoni-managed-store/1\n";
const EMPTY: &str = "empty";
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-managed-store", message)
}
fn recovery_required() -> Error {
    Error::new(
        "library-needs-recovery",
        "Run managed-store recovery before planning or writing",
    )
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredFile {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Song {
    pub installed: host::InstalledSong,
    pub entry: ManifestEntry,
    pub transaction: String,
    pub files: Vec<StoredFile>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub schema: String,
    pub revision: String,
    pub songs: Vec<Song>,
}
impl Catalog {
    pub fn snapshot(&self) -> host::Snapshot {
        host::Snapshot {
            revision: self.revision.clone(),
            songs: self.songs.iter().map(|s| s.installed.clone()).collect(),
        }
    }
}
/// A verified catalog publication and its immutable playback directories.
///
/// Directories remain valid after later replacements or recovery: the store
/// retains transaction payloads. This is a local-store integrity check, not a
/// sandbox against another process modifying files after verification.
#[derive(Clone, Debug)]
pub struct PlaybackSnapshot {
    pub revision: String,
    pub songs: Vec<PlaybackSong>,
}

/// The public playback handoff, without exposing transaction layout to hosts.
#[derive(Clone, Debug)]
pub struct PlaybackSong {
    pub entry: ManifestEntry,
    pub directory: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pending {
    schema: String,
    transaction: String,
    previous_revision: String,
    previous_catalog_sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema: String,
    transaction: String,
    import_digest: String,
    catalog_sha256: String,
    files: Vec<StoredFile>,
}
#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Recovery {
    Clean {
        catalog_revision: String,
    },
    Aborted {
        transaction_id: String,
        catalog_revision: String,
    },
    Committed {
        transaction_id: String,
        catalog_revision: String,
    },
}
/// A handle owns the writer lock for its entire lifetime. Clone catalogs for
/// callers; payload paths remain immutable across later imports/recovery.
pub struct Store {
    root: PathBuf,
    _lock: File,
    #[cfg(test)]
    fault: Option<(Point, bool)>,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Point {
    PendingSynced,
    EvidenceStaged,
    SongStaged,
    CandidateSynced,
    BeforePublish,
    Published,
    DirectorySynced,
    JournalRemoved,
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path).map_err(io)?.sync_all().map_err(io)
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    let b = serde_json::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    if b.len() > JSON_LIMIT {
        return Err(invalid("Oversized store metadata"));
    }
    Ok(b)
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))
}
fn plain(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(io)?;
    if m.file_type().is_symlink() {
        return Err(invalid("Store symlinks are forbidden"));
    }
    Ok(())
}
fn contained(root: &Path, relative: &str) -> Result<PathBuf> {
    assets::relative_path(relative)?;
    let mut p = root.to_path_buf();
    for c in relative.split('/') {
        p.push(c);
        plain(&p)?;
    }
    Ok(p)
}
fn read(root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>> {
    let p = contained(root, relative)?;
    if !fs::metadata(&p).map_err(io)?.is_file() {
        return Err(invalid("Expected regular store file"));
    }
    let mut b = Vec::new();
    File::open(p)
        .map_err(io)?
        .take(limit as u64 + 1)
        .read_to_end(&mut b)
        .map_err(io)?;
    if b.len() > limit {
        return Err(invalid("Oversized store file"));
    }
    Ok(b)
}
fn write_new(root: &Path, relative: &str, bytes: &[u8]) -> Result<()> {
    assets::relative_path(relative)?;
    let mut p = root.to_path_buf();
    let parts = relative.split('/').collect::<Vec<_>>();
    for c in &parts[..parts.len() - 1] {
        p.push(c);
        if !p.try_exists().map_err(io)? {
            fs::create_dir(&p).map_err(io)?;
        }
        plain(&p)?;
        if !p.is_dir() {
            return Err(invalid("Expected store directory"));
        }
    }
    p.push(parts[parts.len() - 1]);
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(p)
        .map_err(io)?;
    f.write_all(bytes).map_err(io)?;
    f.sync_all().map_err(io)
}
fn sync_tree(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root).map_err(io)? {
        let p = entry.map_err(io)?.path();
        plain(&p)?;
        if p.is_dir() {
            sync_tree(&p)?;
        }
    }
    sync_dir(root)
}
fn exists(root: &Path, name: &str) -> Result<bool> {
    // symlink_metadata, including dangling symlinks: never silently treat those
    // as missing and replace/follow an unexpected object.
    match fs::symlink_metadata(root.join(name)) {
        Ok(_) => {
            plain(&root.join(name))?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io(e)),
    }
}
fn catalog_valid(c: &Catalog) -> Result<()> {
    if c.schema != "ddi-danoni-managed-catalog/1"
        || !(c.revision == EMPTY || valid_hash(&c.revision))
        || c.songs.len() > 100000
        || (c.revision == EMPTY && !c.songs.is_empty())
    {
        return Err(invalid("Invalid catalog header"));
    }
    let mut ids = BTreeSet::new();
    for s in &c.songs {
        let i = &s.installed;
        if !valid_hash(&s.transaction)
            || !valid_hash(&i.identity_sha256)
            || !valid_hash(&i.source_identity_sha256)
            || !valid_hash(&i.revision_sha256)
            || i.id != format!("u-{}", i.identity_sha256)
            || s.entry.id != i.id
            || !ids.insert(&i.id)
            || i.selectors.is_empty()
            || i.selectors.len() > 16
            || i.selectors.iter().any(|n| *n >= 16)
            || i.selectors.windows(2).any(|w| w[0] >= w[1])
            || s.files.is_empty()
            || s.files.len() > 16
        {
            return Err(invalid("Invalid managed song identity/inventory"));
        }
        let mut paths = BTreeSet::new();
        for f in &s.files {
            assets::relative_path(&f.path)?;
            if f.path.contains('/')
                || !valid_hash(&f.sha256)
                || f.bytes > MEDIA_LIMIT
                || !paths.insert(f.path.to_lowercase())
            {
                return Err(invalid("Invalid playback inventory"));
            }
        }
        for name in [&s.entry.chart, &s.entry.music]
            .into_iter()
            .chain(s.entry.banner.iter())
        {
            if !s.files.iter().any(|f| f.path == name.as_str()) {
                return Err(invalid("Missing playback asset"));
            }
        }
    }
    Ok(())
}
impl Store {
    /// Creates a new directory only. Never adopts or clears an existing folder.
    pub fn create(root: &Path) -> Result<Self> {
        if !cfg!(unix) {
            return Err(invalid("Managed-store durability currently requires Unix"));
        }
        let parent = root
            .parent()
            .ok_or_else(|| invalid("Store needs a parent"))?
            .canonicalize()
            .map_err(io)?;
        crate::native::external(&parent)?;
        let name = root
            .file_name()
            .ok_or_else(|| invalid("Store needs a directory name"))?;
        let root = parent.join(name);
        fs::create_dir(&root).map_err(io)?;
        write_new(&root, "format", FORMAT)?;
        fs::create_dir(root.join("transactions")).map_err(io)?;
        write_new(
            &root,
            "catalog.json",
            &encode(&Catalog {
                schema: "ddi-danoni-managed-catalog/1".into(),
                revision: EMPTY.into(),
                songs: Vec::new(),
            })?,
        )?;
        write_new(&root, "writer.lock", b"")?;
        sync_dir(&root.join("transactions"))?;
        sync_dir(&root)?;
        sync_dir(&parent)?;
        Self::open(&root)
    }
    pub fn open(root: &Path) -> Result<Self> {
        if !cfg!(unix) {
            return Err(invalid("Managed-store durability currently requires Unix"));
        }
        let absolute = if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir().map_err(io)?.join(root)
        };
        for p in absolute.ancestors() {
            plain(p)?;
        }
        let root = root.canonicalize().map_err(io)?;
        crate::native::external(&root)?;
        if read(&root, "format", 128)? != FORMAT {
            return Err(invalid("Unknown store format"));
        }
        let lock = contained(&root, "writer.lock")?;
        if !lock.is_file() {
            return Err(invalid("Lock is not a regular file"));
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock)
            .map_err(io)?;
        lock.try_lock()
            .map_err(|e| Error::new("library-store-busy", e.to_string()))?;
        let s = Self {
            root,
            _lock: lock,
            #[cfg(test)]
            fault: None,
        };
        s.raw_catalog()?;
        let t = contained(&s.root, "transactions")?;
        if !t.is_dir() {
            return Err(invalid("Missing transactions directory"));
        }
        Ok(s)
    }
    fn hit(&mut self, point: Point) -> Result<()> {
        #[cfg(test)]
        if self.fault.is_some_and(|(p, _)| p == point) {
            let (_, crash) = self.fault.take().unwrap();
            if crash {
                std::process::exit(77);
            }
            return Err(Error::new("injected-store-failure", format!("{point:?}")));
        }
        let _ = point;
        Ok(())
    }
    fn raw_catalog(&self) -> Result<(Catalog, Vec<u8>)> {
        let bytes = read(&self.root, "catalog.json", JSON_LIMIT)?;
        let c: Catalog = decode(&bytes)?;
        catalog_valid(&c)?;
        Ok((c, bytes))
    }
    fn idle(&self) -> Result<()> {
        if exists(&self.root, "pending.json")?
            || exists(&self.root, "pending.new")?
            || exists(&self.root, "catalog.new")?
        {
            return Err(recovery_required());
        }
        Ok(())
    }
    /// Metadata-only listing; does not load audio/evidence into memory. Blocked
    /// during recovery. Use `verify` for a complete inventory audit.
    pub fn catalog(&self) -> Result<Catalog> {
        self.idle()?;
        let (c, bytes) = self.raw_catalog()?;
        self.bound_catalog(&c, &bytes)?;
        Ok(c)
    }
    fn receipt(&self, id: &str) -> Result<Receipt> {
        if !valid_hash(id) {
            return Err(invalid("Invalid transaction ID"));
        }
        let r: Receipt = decode(&read(
            &self.root,
            &format!("transactions/{id}/receipt.json"),
            JSON_LIMIT,
        )?)?;
        if r.schema != "ddi-danoni-managed-transaction/1"
            || r.transaction != id
            || !valid_hash(&r.import_digest)
            || !valid_hash(&r.catalog_sha256)
            || r.files.len() > 32768
        {
            return Err(invalid("Invalid transaction receipt"));
        }
        let mut paths = BTreeSet::new();
        for f in &r.files {
            assets::relative_path(&f.path)?;
            if !valid_hash(&f.sha256)
                || f.bytes > 512 * 1024 * 1024
                || !paths.insert(f.path.to_lowercase())
            {
                return Err(invalid("Invalid transaction inventory"));
            }
        }
        Ok(r)
    }
    fn bound_catalog(&self, c: &Catalog, bytes: &[u8]) -> Result<()> {
        if c.revision == EMPTY {
            return Ok(());
        }
        let r = self.receipt(&c.revision)?;
        let candidate = read(
            &self.root,
            &format!("transactions/{}/catalog.json", c.revision),
            JSON_LIMIT,
        )?;
        if r.catalog_sha256 != sha256(bytes) || candidate != bytes {
            return Err(invalid("Catalog is not the receipted candidate"));
        }
        Ok(())
    }
    fn verify_catalog(&self, c: &Catalog, bytes: &[u8]) -> Result<()> {
        self.bound_catalog(c, bytes)?;
        let ids = c
            .songs
            .iter()
            .map(|s| s.transaction.as_str())
            .chain((c.revision != EMPTY).then_some(c.revision.as_str()))
            .collect::<BTreeSet<_>>();
        for id in ids {
            let r = self.receipt(id)?;
            let base = format!("transactions/{id}");
            for f in &r.files {
                let b = read(&self.root, &format!("{base}/{}", f.path), f.bytes)?;
                if b.len() != f.bytes || sha256(&b) != f.sha256 {
                    return Err(invalid("Transaction inventory hash mismatch"));
                }
            }
            let archived = read(&self.root, &format!("{base}/catalog.json"), JSON_LIMIT)?;
            if sha256(&archived) != r.catalog_sha256 {
                return Err(invalid("Archived catalog hash mismatch"));
            }
        }
        for s in &c.songs {
            let r = self.receipt(&s.transaction)?;
            let summary_path = format!("songs/{}/summary.json", s.installed.id);
            let summary_bytes = read(
                &self.root,
                &format!("transactions/{}/{summary_path}", s.transaction),
                JSON_LIMIT,
            )?;
            if !r.files.iter().any(|f| {
                f.path == summary_path
                    && f.sha256 == sha256(&summary_bytes)
                    && f.bytes == summary_bytes.len()
            }) {
                return Err(invalid("Missing receipted song summary"));
            }
            let summary: serde_json::Value = decode(&summary_bytes)?;
            let expected = serde_json::json!({"identitySha256":s.installed.identity_sha256,"sourceIdentitySha256":s.installed.source_identity_sha256,"revisionSha256":s.installed.revision_sha256,"selectors":s.installed.selectors,"entry":s.entry,"files":s.files});
            for key in [
                "identitySha256",
                "sourceIdentitySha256",
                "revisionSha256",
                "selectors",
                "entry",
                "files",
            ] {
                if summary.get(key) != expected.get(key) {
                    return Err(invalid("Catalog row disagrees with admission summary"));
                }
            }
            for f in &s.files {
                let p = format!("songs/{}/{}", s.installed.id, f.path);
                if !r
                    .files
                    .iter()
                    .any(|e| e.path == p && e.sha256 == f.sha256 && e.bytes == f.bytes)
                {
                    return Err(invalid("Song is not bound to its transaction inventory"));
                }
            }
        }
        Ok(())
    }
    pub fn verify(&self) -> Result<Catalog> {
        self.idle()?;
        let (c, bytes) = self.raw_catalog()?;
        self.verify_catalog(&c, &bytes)?;
        Ok(c)
    }
    /// Verifies the published inventory before handing paths to a player.
    /// Unpublished/cancelled transactions are never listed, and a store needing
    /// recovery or containing corrupted published assets fails closed.
    pub fn playback_snapshot(&self) -> Result<PlaybackSnapshot> {
        let catalog = self.verify()?;
        let mut songs = Vec::with_capacity(catalog.songs.len());
        for song in catalog.songs {
            let directory = contained(
                &self.root,
                &format!(
                    "transactions/{}/songs/{}",
                    song.transaction, song.installed.id
                ),
            )?;
            if !directory.is_dir() {
                return Err(invalid("Expected playback directory"));
            }
            songs.push(PlaybackSong {
                entry: song.entry,
                directory,
            });
        }
        Ok(PlaybackSnapshot {
            revision: catalog.revision,
            songs,
        })
    }
    /// Reads one flat playback file, checking its saved size/hash. Catalog rows
    /// from older snapshots remain readable; garbage collection is not implied.
    pub fn read_asset(&self, song: &Song, name: &str, maximum: usize) -> Result<Vec<u8>> {
        catalog_valid(&Catalog {
            schema: "ddi-danoni-managed-catalog/1".into(),
            revision: song.transaction.clone(),
            songs: vec![song.clone()],
        })?;
        let f = song
            .files
            .iter()
            .find(|f| f.path == name)
            .ok_or_else(|| invalid("Unknown playback file"))?;
        if f.bytes > maximum {
            return Err(invalid("Playback file exceeds caller budget"));
        }
        let r = self.receipt(&song.transaction)?;
        let path = format!("songs/{}/{name}", song.installed.id);
        if !r
            .files
            .iter()
            .any(|e| e.path == path && e.sha256 == f.sha256 && e.bytes == f.bytes)
        {
            return Err(invalid("Playback file is not receipted"));
        }
        let b = read(
            &self.root,
            &format!("transactions/{}/{path}", song.transaction),
            f.bytes,
        )?;
        if b.len() != f.bytes || sha256(&b) != f.sha256 {
            return Err(invalid("Playback hash mismatch"));
        }
        Ok(b)
    }
    /// Recovery never deletes payloads. A pending old catalog means abort; an
    /// exactly receipted new catalog means commit. Any third state fails closed.
    pub fn recover(&mut self) -> Result<Recovery> {
        let (c, bytes) = self.raw_catalog()?;
        if !exists(&self.root, "pending.json")? {
            self.verify_catalog(&c, &bytes)?;
            // These reserved scratch files cannot have published without a
            // durable pending journal. Remove only these files, not directories.
            for n in ["pending.new", "catalog.new"] {
                if exists(&self.root, n)? {
                    fs::remove_file(self.root.join(n)).map_err(io)?;
                }
            }
            sync_dir(&self.root)?;
            return Ok(Recovery::Clean {
                catalog_revision: c.revision,
            });
        }
        let p: Pending = decode(&read(&self.root, "pending.json", JSON_LIMIT)?)?;
        if p.schema != "ddi-danoni-managed-pending/1"
            || !valid_hash(&p.transaction)
            || !valid_hash(&p.previous_catalog_sha256)
            || !(p.previous_revision == EMPTY || valid_hash(&p.previous_revision))
        {
            return Err(invalid("Invalid recovery journal"));
        }
        let committed = c.revision == p.transaction;
        if !committed
            && (c.revision != p.previous_revision || sha256(&bytes) != p.previous_catalog_sha256)
        {
            return Err(invalid("Recovery catalog matches neither journal state"));
        }
        self.verify_catalog(&c, &bytes)?;
        // Stabilize a possibly published rename before retiring recovery data.
        sync_dir(&self.root)?;
        for n in ["catalog.new", "pending.new"] {
            if exists(&self.root, n)? {
                fs::remove_file(self.root.join(n)).map_err(io)?;
            }
        }
        fs::remove_file(self.root.join("pending.json")).map_err(io)?;
        sync_dir(&self.root)?;
        if committed {
            Ok(Recovery::Committed {
                transaction_id: p.transaction,
                catalog_revision: c.revision,
            })
        } else {
            Ok(Recovery::Aborted {
                transaction_id: p.transaction,
                catalog_revision: c.revision,
            })
        }
    }
}
pub struct InstallTransaction<'a> {
    store: &'a mut Store,
    plan: host::PlanSummary,
    catalog: Catalog,
    id: String,
    inventory: Vec<StoredFile>,
    staged: BTreeSet<String>,
    evidence: bool,
    publication_started: bool,
    finished: bool,
}
impl LibraryStore for Store {
    type Transaction<'a> = InstallTransaction<'a>;
    fn check_revision(&mut self, expected: &str) -> Result<()> {
        if self.catalog()?.revision != expected {
            return Err(Error::new(
                "library-stale-catalog",
                "Catalog changed; replan import",
            ));
        }
        Ok(())
    }
    fn begin<'a>(&'a mut self, plan: &host::PlanSummary) -> Result<InstallTransaction<'a>> {
        let catalog = self.catalog()?;
        if catalog.revision != plan.expected_catalog_revision {
            return Err(Error::new(
                "library-stale-catalog",
                "Catalog changed; replan import",
            ));
        }
        if plan.schema != "ddi-danoni-install-plan/1"
            || !valid_hash(&plan.import_digest)
            || plan.operations.len() > 16
        {
            return Err(invalid("Invalid installation plan"));
        }
        let mut unique = BTreeSet::new();
        for op in &plan.operations {
            let old = catalog.songs.iter().find(|s| s.installed.id == op.id);
            if !unique.insert(&op.id)
                || !valid_hash(&op.revision_sha256)
                || old.map(|s| &s.installed.revision_sha256) != op.previous_revision_sha256.as_ref()
                || (op.decision == Decision::Add && old.is_some())
                || (op.decision != Decision::Add && old.is_none())
            {
                return Err(invalid("Plan preconditions do not match actual catalog"));
            }
        }
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = sha256(
            format!(
                "{}:{}:{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| invalid(e.to_string()))?
                    .as_nanos(),
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            )
            .as_bytes(),
        );
        fs::create_dir(self.root.join("transactions").join(&id)).map_err(io)?;
        sync_dir(&self.root.join("transactions"))?;
        let previous = read(&self.root, "catalog.json", JSON_LIMIT)?;
        write_new(
            &self.root,
            "pending.new",
            &encode(&Pending {
                schema: "ddi-danoni-managed-pending/1".into(),
                transaction: id.clone(),
                previous_revision: catalog.revision.clone(),
                previous_catalog_sha256: sha256(&previous),
            })?,
        )?;
        fs::rename(
            self.root.join("pending.new"),
            self.root.join("pending.json"),
        )
        .map_err(io)?;
        sync_dir(&self.root)?;
        self.hit(Point::PendingSynced)?;
        let mut tx = InstallTransaction {
            store: self,
            plan: plan.clone(),
            catalog,
            id,
            inventory: Vec::new(),
            staged: BTreeSet::new(),
            evidence: false,
            publication_started: false,
            finished: false,
        };
        tx.write("plan.json", &encode(plan)?)?;
        Ok(tx)
    }
}
impl InstallTransaction<'_> {
    fn base(&self) -> PathBuf {
        self.store.root.join("transactions").join(&self.id)
    }
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<()> {
        write_new(&self.base(), path, bytes)?;
        self.inventory.push(StoredFile {
            path: path.into(),
            sha256: sha256(bytes),
            bytes: bytes.len(),
        });
        Ok(())
    }
    fn prepare_commit(&mut self) -> Result<()> {
        let required = self
            .plan
            .operations
            .iter()
            .filter(|o| matches!(o.decision, Decision::Add | Decision::Replace))
            .map(|o| o.id.clone())
            .collect::<BTreeSet<_>>();
        if !self.evidence || self.staged != required {
            return Err(invalid("Incomplete installation transaction"));
        }
        self.catalog.revision = self.id.clone();
        self.catalog
            .songs
            .sort_by(|a, b| a.installed.id.cmp(&b.installed.id));
        catalog_valid(&self.catalog)?;
        let candidate = encode(&self.catalog)?;
        write_new(&self.base(), "catalog.json", &candidate)?;
        write_new(
            &self.base(),
            "receipt.json",
            &encode(&Receipt {
                schema: "ddi-danoni-managed-transaction/1".into(),
                transaction: self.id.clone(),
                import_digest: self.plan.import_digest.clone(),
                catalog_sha256: sha256(&candidate),
                files: self.inventory.clone(),
            })?,
        )?;
        // Apply the reader's limits/case-collision rules before anything can
        // become visible, including evidence supplied by a custom AssetSource.
        self.store.receipt(&self.id)?;
        sync_tree(&self.base())?;
        self.store.hit(Point::CandidateSynced)?;
        write_new(&self.store.root, "catalog.new", &candidate)?;
        // Ensure both the durable recovery record and candidate exist before
        // the sole visibility operation. All content already lives at final paths.
        sync_dir(&self.store.root)?;
        self.store.hit(Point::BeforePublish)?;
        Ok(())
    }
}
impl Transaction for InstallTransaction<'_> {
    fn stage_evidence(&mut self, import: &PreparedImport) -> Result<()> {
        if self.evidence
            || self.finished
            || self.publication_started
            || import.summary().digest != self.plan.import_digest
        {
            return Err(invalid("Evidence does not match transaction"));
        }
        self.write("import.json", &encode(import.summary())?)?;
        for f in import.evidence() {
            self.write(&format!("evidence/{}", f.path), &f.bytes)?;
        }
        self.evidence = true;
        self.store.hit(Point::EvidenceStaged)
    }
    fn stage_song(&mut self, song: &PreparedSong, operation: &host::Operation) -> Result<()> {
        let s = song.summary();
        let op = self
            .plan
            .operations
            .iter()
            .find(|o| o.id == operation.id)
            .ok_or_else(|| invalid("Unplanned song"))?;
        if self.finished
            || self.publication_started
            || !self.evidence
            || !matches!(op.decision, Decision::Add | Decision::Replace)
            || op.revision_sha256 != s.revision_sha256
            || op.id != s.entry.id
            || self.staged.contains(&op.id)
        {
            return Err(invalid("Unplanned/repeated song revision"));
        }
        if let Some(old) = self
            .catalog
            .songs
            .iter()
            .find(|e| e.installed.id == s.entry.id)
            && (old.installed.source_identity_sha256 != s.source_identity_sha256
                || old.installed.identity_sha256 != s.identity_sha256
                || old
                    .installed
                    .selectors
                    .iter()
                    .any(|n| !s.selectors.contains(n)))
        {
            return Err(invalid(
                "Replacement would change identity or remove charts",
            ));
        }
        for f in song.files() {
            self.write(&format!("songs/{}/{}", s.entry.id, f.path), &f.bytes)?;
        }
        self.write(&format!("songs/{}/summary.json", s.entry.id), &encode(s)?)?;
        self.catalog.songs.retain(|e| e.installed.id != s.entry.id);
        self.catalog.songs.push(Song {
            installed: host::InstalledSong {
                id: s.entry.id.clone(),
                source_identity_sha256: s.source_identity_sha256.clone(),
                identity_sha256: s.identity_sha256.clone(),
                revision_sha256: s.revision_sha256.clone(),
                selectors: s.selectors.clone(),
            },
            entry: s.entry.clone(),
            transaction: self.id.clone(),
            files: s
                .files
                .iter()
                .map(|f| StoredFile {
                    path: f.path.clone(),
                    sha256: f.sha256.clone(),
                    bytes: f.bytes,
                })
                .collect(),
        });
        self.staged.insert(s.entry.id.clone());
        self.store.hit(Point::SongStaged)
    }
    fn commit(&mut self) -> CommitOutcome {
        if self.publication_started {
            return if self.finished {
                CommitOutcome::Committed {
                    catalog_revision: self.id.clone(),
                }
            } else {
                CommitOutcome::NeedsRecovery {
                    transaction_id: self.id.clone(),
                    reason: "Publication already attempted; recovery required".into(),
                }
            };
        }
        if self.finished {
            return CommitOutcome::Rejected {
                error: invalid("Transaction was rolled back"),
            };
        }
        if let Err(error) = self.prepare_commit() {
            return CommitOutcome::Rejected { error };
        }
        self.publication_started = true;
        let result = (|| {
            fs::rename(
                self.store.root.join("catalog.new"),
                self.store.root.join("catalog.json"),
            )
            .map_err(io)?;
            self.store.hit(Point::Published)?;
            sync_dir(&self.store.root)?;
            self.store.hit(Point::DirectorySynced)?;
            fs::remove_file(self.store.root.join("pending.json")).map_err(io)?;
            self.store.hit(Point::JournalRemoved)?;
            sync_dir(&self.store.root)
        })();
        match result {
            Ok(()) => {
                self.finished = true;
                CommitOutcome::Committed {
                    catalog_revision: self.id.clone(),
                }
            }
            Err(e) => CommitOutcome::NeedsRecovery {
                transaction_id: self.id.clone(),
                reason: e.to_string(),
            },
        }
    }
    fn rollback(&mut self) -> Result<()> {
        if self.publication_started {
            return Err(recovery_required());
        }
        if self.finished {
            return Ok(());
        }
        // Verify the old catalog and journal relationship before retiring the
        // journal. Recovery retains staged payloads, so repeated rollback is safe.
        self.store.recover()?;
        self.finished = true;
        Ok(())
    }
}
impl Drop for InstallTransaction<'_> {
    fn drop(&mut self) {
        if !self.finished && !self.publication_started {
            let _ = self.rollback();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_fixture::{fixture, validated};
    fn temp(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().canonicalize().unwrap().join(format!(
            "ddi-managed-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn import(title: &str, split: bool) -> PreparedImport {
        let (m, b) = fixture(&[0, 1], split, title);
        validated(&m, &b)
    }
    fn install(
        store: &mut Store,
        p: &PreparedImport,
        policy: host::DuplicatePolicy,
    ) -> Result<host::Outcome> {
        let plan = host::plan(p, &store.catalog()?.snapshot(), policy)?;
        host::install(p, &plan, store, &mut |_| Ok(()))
    }
    fn audit_assets(store: &Store, c: &Catalog) {
        for song in &c.songs {
            for f in &song.files {
                let bytes = store.read_asset(song, &f.path, MEDIA_LIMIT).unwrap();
                assert_eq!(sha256(&bytes), f.sha256);
            }
        }
    }
    #[test]
    fn durable_add_replace_old_readers_and_noop() {
        let root = temp("reopen");
        let mut store = Store::create(&root).unwrap();
        assert!(Store::create(&root).is_err());
        assert_eq!(Store::open(&root).err().unwrap().code, "library-store-busy");
        let a = import("First", true);
        assert!(matches!(
            install(&mut store, &a, host::DuplicatePolicy::Reject).unwrap(),
            host::Outcome::Installed { added: 2, .. }
        ));
        let old = store.verify().unwrap();
        let stale = host::plan(
            &a,
            &host::Snapshot {
                revision: EMPTY.into(),
                songs: vec![],
            },
            host::DuplicatePolicy::Reject,
        )
        .unwrap();
        assert_eq!(
            host::install(&a, &stale, &mut store, &mut |_| Ok(()))
                .unwrap_err()
                .code,
            "library-stale-catalog"
        );
        let dir_count = fs::read_dir(root.join("transactions")).unwrap().count();
        assert!(matches!(
            install(&mut store, &a, host::DuplicatePolicy::Reject).unwrap(),
            host::Outcome::Unchanged { .. }
        ));
        assert_eq!(
            dir_count,
            fs::read_dir(root.join("transactions")).unwrap().count()
        );
        let b = import("Changed", true);
        assert_eq!(
            install(&mut store, &b, host::DuplicatePolicy::Reject)
                .unwrap_err()
                .code,
            "library-import-conflict"
        );
        assert!(matches!(
            install(&mut store, &b, host::DuplicatePolicy::KeepExisting).unwrap(),
            host::Outcome::Unchanged { .. }
        ));
        assert!(matches!(
            install(&mut store, &b, host::DuplicatePolicy::Replace).unwrap(),
            host::Outcome::Installed { replaced: 2, .. }
        ));
        audit_assets(&store, &old);
        drop(store);
        let store = Store::open(&root).unwrap();
        let c = store.verify().unwrap();
        assert_eq!(c.songs.len(), 2);
        assert!(c.songs.iter().all(|s| s.entry.title == "Changed"));
        audit_assets(&store, &c);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn playback_snapshot_is_published_verified_and_survives_replacement() {
        let root = temp("playback");
        let mut store = Store::create(&root).unwrap();
        assert!(store.playback_snapshot().unwrap().songs.is_empty());
        install(
            &mut store,
            &import("First", false),
            host::DuplicatePolicy::Reject,
        )
        .unwrap();
        let old = store.playback_snapshot().unwrap();
        assert_eq!(old.songs.len(), 1);
        let chart = old.songs[0]
            .entry
            .chart
            .to_path(&old.songs[0].directory)
            .unwrap();
        let bytes = fs::read(&chart).unwrap();
        install(
            &mut store,
            &import("Replacement", false),
            host::DuplicatePolicy::Replace,
        )
        .unwrap();
        let new = store.playback_snapshot().unwrap();
        assert_ne!(old.revision, new.revision);
        assert_ne!(old.songs[0].directory, new.songs[0].directory);
        assert_eq!(old.songs[0].entry.id, new.songs[0].entry.id);
        assert_eq!(fs::read(chart).unwrap(), bytes);
        write_new(&root, "pending.new", b"interrupted").unwrap();
        assert_eq!(
            store.playback_snapshot().unwrap_err().code,
            "library-needs-recovery"
        );
        store.recover().unwrap();
        let current = &new.songs[0];
        fs::write(
            current.entry.music.to_path(&current.directory).unwrap(),
            b"corrupted",
        )
        .unwrap();
        assert!(store.playback_snapshot().is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_drop_and_real_catalog_preconditions() {
        let root = temp("cancel");
        let mut store = Store::create(&root).unwrap();
        let a = import("First", true);
        let plan = host::plan(
            &a,
            &store.catalog().unwrap().snapshot(),
            host::DuplicatePolicy::Reject,
        )
        .unwrap();
        let result = host::install(&a, &plan, &mut store, &mut |p| {
            if p.stage == host::Stage::Songs && p.completed == 1 {
                Err(Error::new("cancelled", "test"))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.unwrap_err().code, "cancelled");
        assert!(store.verify().unwrap().songs.is_empty());
        {
            let mut tx = store.begin(plan.summary()).unwrap();
            tx.stage_evidence(&a).unwrap();
        }
        assert!(store.verify().unwrap().songs.is_empty());
        install(&mut store, &a, host::DuplicatePolicy::Reject).unwrap();
        let c = store.catalog().unwrap();
        let omitted = host::Snapshot {
            revision: c.revision,
            songs: vec![],
        };
        let forged = host::plan(&a, &omitted, host::DuplicatePolicy::Reject).unwrap();
        assert!(store.begin(forged.summary()).is_err());
        let (m, b) = fixture(&[0], false, "Fewer charts");
        let partial = validated(&m, &b);
        // Different grouping is another identity here; use a shared-music
        // baseline to exercise the actual same-song selector-removal guard.
        let root2 = temp("subset");
        let mut s = Store::create(&root2).unwrap();
        install(
            &mut s,
            &import("Shared", false),
            host::DuplicatePolicy::Reject,
        )
        .unwrap();
        assert_eq!(
            install(&mut s, &partial, host::DuplicatePolicy::Replace)
                .unwrap_err()
                .code,
            "library-chart-removal"
        );
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(root2).unwrap();
    }
    #[test]
    fn failures_before_and_after_publication_are_distinct() {
        for point in [
            Point::PendingSynced,
            Point::EvidenceStaged,
            Point::SongStaged,
            Point::CandidateSynced,
            Point::BeforePublish,
            Point::Published,
            Point::DirectorySynced,
            Point::JournalRemoved,
        ] {
            let root = temp("fault");
            let mut s = Store::create(&root).unwrap();
            let a = import("Fault", true);
            s.fault = Some((point, false));
            let result = install(&mut s, &a, host::DuplicatePolicy::Reject);
            let published = matches!(
                point,
                Point::Published | Point::DirectorySynced | Point::JournalRemoved
            );
            if published {
                assert!(matches!(
                    result.unwrap(),
                    host::Outcome::NeedsRecovery { .. }
                ));
            } else {
                assert!(result.is_err());
            }
            drop(s);
            let mut s = Store::open(&root).unwrap();
            s.recover().unwrap();
            let c = s.verify().unwrap();
            assert_eq!(c.songs.len(), if published { 2 } else { 0 }, "{point:?}");
            audit_assets(&s, &c);
            s.recover().unwrap();
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn repeated_commit_cannot_be_mistaken_for_rejection_or_rollback() {
        for fault in [None, Some((Point::Published, false))] {
            let root = temp("reentry");
            let mut store = Store::create(&root).unwrap();
            let a = import("Reentry", false);
            let plan = host::plan(
                &a,
                &store.catalog().unwrap().snapshot(),
                host::DuplicatePolicy::Reject,
            )
            .unwrap();
            store.fault = fault;
            let mut tx = store.begin(plan.summary()).unwrap();
            tx.stage_evidence(&a).unwrap();
            tx.stage_song(&a.songs()[0], &plan.summary().operations[0])
                .unwrap();
            for _ in 0..2 {
                let outcome = tx.commit();
                if fault.is_some() {
                    assert!(matches!(outcome, CommitOutcome::NeedsRecovery { .. }));
                } else {
                    assert!(matches!(outcome, CommitOutcome::Committed { .. }));
                }
            }
            assert!(tx.rollback().is_err());
            drop(tx);
            store.recover().unwrap();
            assert_eq!(store.verify().unwrap().songs.len(), 1);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn recovery_refuses_a_third_catalog_state() {
        let root = temp("journal");
        let mut store = Store::create(&root).unwrap();
        store.fault = Some((Point::PendingSynced, false));
        assert!(
            install(
                &mut store,
                &import("Journal", false),
                host::DuplicatePolicy::Reject
            )
            .is_err()
        );
        let original = fs::read(root.join("pending.json")).unwrap();
        let mut p: Pending = decode(&original).unwrap();
        p.previous_catalog_sha256 = sha256(b"other state");
        fs::write(root.join("pending.json"), encode(&p).unwrap()).unwrap();
        assert!(store.recover().is_err());
        assert!(root.join("pending.json").exists());
        fs::write(root.join("pending.json"), original).unwrap();
        assert!(matches!(store.recover().unwrap(), Recovery::Aborted { .. }));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn crash_child() {
        let Some(root) = std::env::var_os("DDI_MANAGED_CRASH_ROOT") else {
            return;
        };
        let n: usize = std::env::var("DDI_MANAGED_CRASH_POINT")
            .unwrap()
            .parse()
            .unwrap();
        let points = [
            Point::PendingSynced,
            Point::EvidenceStaged,
            Point::SongStaged,
            Point::CandidateSynced,
            Point::BeforePublish,
            Point::Published,
            Point::DirectorySynced,
            Point::JournalRemoved,
        ];
        let mut s = Store::open(Path::new(&root)).unwrap();
        s.fault = Some((points[n], true));
        install(
            &mut s,
            &import("After crash", true),
            host::DuplicatePolicy::Replace,
        )
        .unwrap();
        panic!("Crash point was not reached");
    }
    #[test]
    fn process_exit_recovery_preserves_all_old_or_all_new_songs() {
        for n in 0..8 {
            let root = temp("crash");
            let mut s = Store::create(&root).unwrap();
            install(
                &mut s,
                &import("Before crash", true),
                host::DuplicatePolicy::Reject,
            )
            .unwrap();
            let old = s.verify().unwrap();
            drop(s);
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "managed_store::tests::crash_child",
                    "--nocapture",
                ])
                .env("DDI_MANAGED_CRASH_ROOT", &root)
                .env("DDI_MANAGED_CRASH_POINT", n.to_string())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(77));
            let mut s = Store::open(&root).unwrap();
            let recovered = s.recover().unwrap();
            let c = s.verify().unwrap();
            assert_eq!(c.songs.len(), 2);
            assert!(c.songs.iter().all(|x| x.entry.title
                == if n >= 5 {
                    "After crash"
                } else {
                    "Before crash"
                }));
            if n >= 5 && n != 7 {
                assert!(matches!(recovered, Recovery::Committed { .. }));
            }
            if n < 5 {
                assert!(matches!(recovered, Recovery::Aborted { .. }));
            }
            audit_assets(&s, &old);
            audit_assets(&s, &c);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn corruption_and_unsafe_files_fail_closed() {
        let root = temp("corrupt");
        let mut s = Store::create(&root).unwrap();
        s.fault = Some((Point::Published, false));
        assert!(matches!(
            install(
                &mut s,
                &import("Corrupt", false),
                host::DuplicatePolicy::Reject
            )
            .unwrap(),
            host::Outcome::NeedsRecovery { .. }
        ));
        let c = s.raw_catalog().unwrap().0;
        let song = &c.songs[0];
        let path = root.join(format!(
            "transactions/{}/songs/{}/{}",
            song.transaction, song.entry.id, song.entry.music
        ));
        fs::write(&path, b"broken").unwrap();
        assert!(s.recover().is_err());
        assert!(root.join("pending.json").exists());
        assert!(s.catalog().is_err());
        assert!(
            s.read_asset(song, song.entry.music.as_str(), MEDIA_LIMIT)
                .is_err()
        );
        drop(s);
        fs::remove_dir_all(&root).unwrap();
        let unknown = temp("unknown");
        fs::create_dir(&unknown).unwrap();
        fs::write(unknown.join("keep"), b"unchanged").unwrap();
        assert!(Store::create(&unknown).is_err());
        assert!(Store::open(&unknown).is_err());
        assert_eq!(fs::read(unknown.join("keep")).unwrap(), b"unchanged");
        fs::remove_dir_all(unknown).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_and_unjournaled_scratch_are_not_followed() {
        use std::os::unix::fs::symlink;
        let root = temp("links");
        let mut s = Store::create(&root).unwrap();
        let target = temp("target");
        fs::write(&target, b"outside").unwrap();
        symlink(&target, root.join("pending.new")).unwrap();
        assert!(s.recover().is_err());
        assert_eq!(fs::read(&target).unwrap(), b"outside");
        fs::remove_file(root.join("pending.new")).unwrap();
        fs::write(root.join("pending.new"), b"partial journal").unwrap();
        s.recover().unwrap();
        assert!(!root.join("pending.new").exists());
        install(
            &mut s,
            &import("Links", false),
            host::DuplicatePolicy::Reject,
        )
        .unwrap();
        let song = s.catalog().unwrap().songs.remove(0);
        let audio = root.join(format!(
            "transactions/{}/songs/{}/{}",
            song.transaction, song.entry.id, song.entry.music
        ));
        fs::remove_file(&audio).unwrap();
        symlink(&target, &audio).unwrap();
        assert!(
            s.read_asset(&song, song.entry.music.as_str(), MEDIA_LIMIT)
                .is_err()
        );
        assert!(s.verify().is_err());
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(target).unwrap();
    }
}
