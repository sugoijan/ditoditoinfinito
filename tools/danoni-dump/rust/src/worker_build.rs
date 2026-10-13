//! Trusted, build-time preparation. Never executes a movie, Python or Cargo.
//! Source bytes come from pinned Git objects, not the mutable worktree. Building
//! them is an explicit operator step; a desktop import only loads a bundle.
use crate::{
    Error, Result,
    assets::*,
    native,
    supervisor::{self, WorkerSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
const SOURCE_LIMIT: usize = 256 * 1024 * 1024;
const FILE_LIMIT: usize = 32 * 1024 * 1024;
const WORKER_LIMIT: usize = 256 * 1024 * 1024;
const HOOKS: [(&str, &str); 3] = [
    ("core/src/lib.rs", "\npub mod ddi_snapshot;\n"),
    (
        "core/src/avm1/object/script_object.rs",
        "\ninclude!(\"../../ddi_properties.rs\");\n",
    ),
    (
        "core/src/avm1/runtime.rs",
        "\nimpl Avm1<'_> { pub(crate) fn ddi_halted(&self) -> bool { self.halted } }\n",
    ),
];
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-worker-build", message)
}
fn bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    let m = fs::symlink_metadata(path).map_err(native::io)?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > max as u64 {
        return Err(invalid("Need a bounded regular file"));
    }
    let mut b = Vec::new();
    fs::File::open(path)
        .map_err(native::io)?
        .take(max as u64 + 1)
        .read_to_end(&mut b)
        .map_err(native::io)?;
    if b.len() > max {
        return Err(invalid("File byte limit"));
    }
    Ok(b)
}
struct Output {
    path: PathBuf,
    ready: bool,
}
impl Drop for Output {
    fn drop(&mut self) {
        if !self.ready {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn output(path: &Path) -> Result<Output> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(native::io)?;
    native::external(&parent)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("Output name"))?;
    relative_path(name)?;
    let path = parent.join(name);
    fs::create_dir(&path).map_err(native::io)?;
    Ok(Output { path, ready: false })
}
fn write(root: &Path, path: &str, bytes: &[u8]) -> Result<Value> {
    relative_path(path)?;
    let dest = root.join(path);
    fs::create_dir_all(dest.parent().unwrap()).map_err(native::io)?;
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(dest)
        .map_err(native::io)?;
    f.write_all(bytes).map_err(native::io)?;
    Ok(json!({"path":path,"sha256":sha256(bytes),"bytes":bytes.len()}))
}
fn ready(root: &Path, value: &Value) -> Result<()> {
    write(
        root,
        ".manifest.tmp",
        &serde_json::to_vec_pretty(value).map_err(|e| invalid(e.to_string()))?,
    )?;
    fs::rename(root.join(".manifest.tmp"), root.join("manifest.json")).map_err(native::io)
}
fn git(upstream: &Path) -> Command {
    let mut c = Command::new("git");
    // No replacements, alternate worktree edits, hooks, pager or per-user Git env.
    c.env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args([
            "--no-replace-objects",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-C",
        ])
        .arg(upstream);
    c
}
fn git_output(upstream: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let o = git(upstream)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(native::io)?;
    if !o.status.success() {
        return Err(invalid(format!(
            "Pinned Git objects unavailable: {}",
            String::from_utf8_lossy(&o.stderr)
        )));
    }
    Ok(o.stdout)
}
fn selected(path: &str) -> bool {
    [
        "core/",
        "render/",
        "swf/",
        "flv/",
        "wstr/",
        "video/",
        "tools/asc/",
    ]
    .iter()
    .any(|p| path.starts_with(p))
        || ["Cargo.toml", "Cargo.lock", "LICENSE.md", "README.md"].contains(&path)
}
struct GitChild(Child, bool);
impl Drop for GitChild {
    fn drop(&mut self) {
        if !self.1 {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
/// Copy the reviewed release into a fresh external tree. Uncommitted upstream
/// edits and symlinks are never read; hook additions only touch the owned copy.
pub fn prepare_sources(
    upstream: &Path,
    destination: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    checkpoint()?;
    let upstream = upstream.canonicalize().map_err(native::io)?;
    let revision = git_output(
        &upstream,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", supervisor::REVISION),
        ],
    )?;
    if String::from_utf8_lossy(&revision).trim() != supervisor::REVISION {
        return Err(invalid("Wrong source revision"));
    }
    let tree = git_output(
        &upstream,
        &[
            "ls-tree",
            "-r",
            "-z",
            supervisor::REVISION,
            "--",
            "core",
            "render",
            "swf",
            "flv",
            "wstr",
            "video",
            "tools/asc",
            "Cargo.toml",
            "Cargo.lock",
            "LICENSE.md",
            "README.md",
        ],
    )?;
    if tree.len() > 4 * 1024 * 1024 {
        return Err(invalid("Source tree inventory limit"));
    }
    let mut entries = Vec::new();
    for row in tree.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = std::str::from_utf8(row).map_err(|_| invalid("Non-UTF8 Git path"))?;
        let (header, path) = row
            .split_once('\t')
            .ok_or_else(|| invalid("Git tree entry"))?;
        if !selected(path) {
            continue;
        }
        relative_path(path)?;
        let fields = header.split(' ').collect::<Vec<_>>();
        if fields.len() != 3
            || !["100644", "100755"].contains(&fields[0])
            || fields[1] != "blob"
            || fields[2].len() != 40
            || !fields[2].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("Source must consist of regular pinned blobs"));
        }
        entries.push((path.to_owned(), fields[2].to_owned(), fields[0].to_owned()));
    }
    if entries.is_empty() || entries.len() > 10000 {
        return Err(invalid("Source entry limit"));
    }
    let mut owned = output(destination)?;
    // Objects must already be cached. Preparation never fetches a dependency;
    // populate a sparse clone explicitly before calling this API.
    let mut child = GitChild(
        git(&upstream)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(native::io)?,
        false,
    );
    let mut stdin = child.0.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.0.stdout.take().unwrap());
    let mut inventory = Vec::new();
    let mut originals = BTreeMap::new();
    let mut total = 0usize;
    for (path, blob, mode) in &entries {
        checkpoint()?;
        writeln!(stdin, "{blob}").map_err(native::io)?;
        stdin.flush().map_err(native::io)?;
        let mut header = String::new();
        stdout
            .by_ref()
            .take(256)
            .read_line(&mut header)
            .map_err(native::io)?;
        let f = header.split_whitespace().collect::<Vec<_>>();
        if f.len() != 3 || f[0] != blob || f[1] != "blob" {
            return Err(invalid("Git object response differs"));
        }
        let size = f[2]
            .parse::<usize>()
            .map_err(|_| invalid("Git object size"))?;
        total = total
            .checked_add(size)
            .ok_or_else(|| invalid("Source size overflow"))?;
        if size > FILE_LIMIT || total > SOURCE_LIMIT {
            return Err(invalid("Source byte limit"));
        }
        let mut bytes = vec![0; size];
        stdout.read_exact(&mut bytes).map_err(native::io)?;
        let mut newline = [0];
        stdout.read_exact(&mut newline).map_err(native::io)?;
        if newline != *b"\n" {
            return Err(invalid("Git object separator"));
        }
        originals.insert(
            path.clone(),
            json!({"blob":blob,"mode":mode,"sha256":sha256(&bytes)}),
        );
        if let Some((_, hook)) = HOOKS.iter().find(|(name, _)| name == path) {
            bytes.extend(hook.as_bytes());
        }
        inventory.push(write(&owned.path, &format!("upstream/{path}"), &bytes)?);
        #[cfg(unix)]
        if mode == "100755" {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                owned.path.join("upstream").join(path),
                fs::Permissions::from_mode(0o755),
            )
            .map_err(native::io)?;
        }
    }
    drop(stdin);
    let status = child.0.wait().map_err(native::io)?;
    child.1 = true;
    if !status.success() {
        return Err(invalid("Git object reader failed"));
    }
    for (path, _) in HOOKS {
        if !originals.contains_key(path) {
            return Err(invalid("Missing pinned hook location"));
        }
    }
    if !originals.contains_key("LICENSE.md") {
        return Err(invalid("Missing upstream licence"));
    }
    let reviewed = supervisor::reviewed_sources();
    for (name, dest) in [
        ("core-properties.rs", "upstream/core/src/ddi_properties.rs"),
        ("core-snapshot.rs", "upstream/core/src/ddi_snapshot.rs"),
    ] {
        inventory.push(write(&owned.path, dest, reviewed[name])?);
    }
    // Relative paths make the prepared source directory portable. No shell or
    // TOML escaping of caller-controlled absolute paths is needed.
    let template =
        std::str::from_utf8(reviewed["Cargo.toml.in"]).map_err(|_| invalid("Template UTF8"))?;
    inventory.push(write(
        &owned.path,
        "worker/Cargo.toml",
        template.replace("@UPSTREAM@", "../upstream").as_bytes(),
    )?);
    for name in [
        "Cargo.lock",
        "src/audio.rs",
        "src/main.rs",
        "src/navigator.rs",
    ] {
        inventory.push(write(
            &owned.path,
            &format!("worker/{name}"),
            reviewed[name],
        )?);
    }
    inventory.push(write(
        &owned.path,
        "worker/build-identity.json",
        &serde_json::to_vec_pretty(&supervisor::expected_build_identity())
            .map_err(|e| invalid(e.to_string()))?,
    )?);
    inventory.push(write(
        &owned.path,
        "licenses/DDI-MIT.txt",
        include_bytes!("../../../../LICENSES/MIT.txt"),
    )?);
    inventory.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    checkpoint()?;
    let receipt = json!({"schema":"ddi-danoni-worker-sources/1","producer":"rust-worker-sources/1","build":supervisor::expected_build_identity(),"upstream":{"url":"https://github.com/ruffle-rs/ruffle","revision":supervisor::REVISION,"files":originals},"hooks":HOOKS.iter().map(|(p,h)|json!({"path":p,"appendSha256":sha256(h.as_bytes())})).collect::<Vec<_>>(),"manifest":"worker/Cargo.toml","artifacts":inventory,"scope":"Pinned source preparation only. Cargo compilation is an explicit trusted build step, never part of a song import."});
    ready(&owned.path, &receipt)?;
    owned.ready = true;
    Ok(receipt)
}
fn contained(root: &Path, path: &str, max: usize) -> Result<Vec<u8>> {
    relative_path(path)?;
    let mut p = root.to_path_buf();
    for part in path.split('/') {
        p.push(part);
        if fs::symlink_metadata(&p)
            .map_err(native::io)?
            .file_type()
            .is_symlink()
        {
            return Err(invalid("Bundle/source symlink"));
        }
    }
    bounded(&p, max)
}
fn verify_inventory(
    root: &Path,
    manifest: &Value,
    max: usize,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let rows = manifest["artifacts"]
        .as_array()
        .ok_or_else(|| invalid("Missing artifact inventory"))?;
    if rows.is_empty() || rows.len() > 10000 {
        return Err(invalid("Artifact inventory limit"));
    }
    let mut paths = BTreeMap::new();
    let mut verified = BTreeMap::new();
    let mut total = 0usize;
    for row in rows {
        checkpoint()?;
        let path = row["path"]
            .as_str()
            .ok_or_else(|| invalid("Artifact path"))?;
        let hash = row["sha256"]
            .as_str()
            .ok_or_else(|| invalid("Artifact hash"))?;
        if !valid_hash(hash) || paths.insert(path, ()).is_some() || path == "manifest.json" {
            return Err(invalid("Invalid/duplicate artifact"));
        }
        let bytes = contained(root, path, max)?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| invalid("Inventory size overflow"))?;
        if total > 512 * 1024 * 1024
            || row["bytes"].as_u64() != Some(bytes.len() as u64)
            || hash != sha256(&bytes)
        {
            return Err(invalid("Changed prepared/bundled artifact"));
        }
        verified.insert(path.to_owned(), bytes);
    }
    fn walk(
        root: &Path,
        dir: &Path,
        paths: &BTreeMap<&str, ()>,
        source: bool,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<()> {
        for entry in fs::read_dir(dir).map_err(native::io)? {
            checkpoint()?;
            let path = entry.map_err(native::io)?.path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .ok_or_else(|| invalid("Nonportable artifact path"))?
                .replace('\\', "/");
            let metadata = fs::symlink_metadata(&path).map_err(native::io)?;
            if metadata.file_type().is_symlink() {
                return Err(invalid("Unexpected artifact symlink"));
            }
            // Cargo output is outside the prepared source inventory.
            if source && rel == "worker/target" && metadata.is_dir() {
                continue;
            }
            if metadata.is_dir() {
                walk(root, &path, paths, source, checkpoint)?;
            } else if !metadata.is_file()
                || (rel != "manifest.json" && !paths.contains_key(rel.as_str()))
            {
                return Err(invalid("Uninventoried artifact"));
            }
        }
        Ok(())
    }
    let source = manifest["schema"] == "ddi-danoni-worker-sources/1";
    let required: &[&str] = if source {
        &[
            "worker/Cargo.toml",
            "worker/Cargo.lock",
            "worker/build-identity.json",
            "worker/src/main.rs",
            "worker/src/audio.rs",
            "worker/src/navigator.rs",
            "upstream/LICENSE.md",
            "upstream/Cargo.toml",
            "upstream/core/src/ddi_properties.rs",
            "upstream/core/src/ddi_snapshot.rs",
            "licenses/DDI-MIT.txt",
        ]
    } else {
        &[
            "ddi-ruffle-probe",
            "build-identity.json",
            "worker.sb",
            "licenses/Ruffle-LICENSE.md",
            "licenses/DDI-MIT.txt",
            "provenance/Cargo.lock",
            "provenance/source-manifest.json",
        ]
    };
    if required.iter().any(|p| !paths.contains_key(p)) {
        return Err(invalid("Incomplete artifact inventory"));
    }
    walk(root, root, &paths, source, checkpoint)?;
    Ok(verified)
}
fn artifact<'a>(files: &'a BTreeMap<String, Vec<u8>>, path: &str) -> Result<&'a [u8]> {
    files
        .get(path)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid("Missing bound artifact"))
}
fn manifest(root: &Path) -> Result<Value> {
    json(&contained(root, "manifest.json", 16 * 1024 * 1024)?)
}
fn platform_binary(bytes: &[u8]) -> Result<()> {
    // The currently implemented confinement backend is macOS. Universal and
    // other-platform bundles need their own reviewed installation contract.
    if !cfg!(target_os = "macos") {
        return Err(Error::new(
            "unsupported-native-confinement",
            "Worker bundles currently target macOS",
        ));
    }
    let cpu = if cfg!(target_arch = "aarch64") {
        0x0100000cu32
    } else if cfg!(target_arch = "x86_64") {
        0x01000007
    } else {
        return Err(invalid("Unsupported worker architecture"));
    };
    if bytes.len() < 32
        || bytes[..4] != [0xcf, 0xfa, 0xed, 0xfe]
        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != cpu
        || u32::from_le_bytes(bytes[12..16].try_into().unwrap()) != 2
    {
        return Err(invalid("Need a native Mach-O executable for this host"));
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageJob {
    pub schema: String,
    pub sources: PathBuf,
    pub executable: PathBuf,
    pub executable_sha256: String,
}
/// Package an explicitly selected, hash-bound build result. This verifies source
/// and package inventories; it does not attest which compiler produced a binary.
pub fn package_worker(
    sources: &Path,
    executable: &Path,
    executable_sha256: &str,
    destination: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    checkpoint()?;
    let sources = sources.canonicalize().map_err(native::io)?;
    native::external(&sources)?;
    let source_bytes = contained(&sources, "manifest.json", 16 * 1024 * 1024)?;
    let source: Value = json(&source_bytes)?;
    if source["schema"] != "ddi-danoni-worker-sources/1"
        || source["build"] != supervisor::expected_build_identity()
    {
        return Err(invalid("Unreviewed prepared source identity"));
    }
    let verified = verify_inventory(&sources, &source, FILE_LIMIT, checkpoint)?;
    if source["upstream"]["revision"] != supervisor::REVISION {
        return Err(invalid("Prepared upstream revision differs"));
    }
    let reviewed = supervisor::reviewed_sources();
    for name in [
        "Cargo.lock",
        "src/audio.rs",
        "src/main.rs",
        "src/navigator.rs",
    ] {
        if artifact(&verified, &format!("worker/{name}"))? != reviewed[name] {
            return Err(invalid("Prepared worker source differs"));
        }
    }
    for (name, dest) in [
        ("core-properties.rs", "upstream/core/src/ddi_properties.rs"),
        ("core-snapshot.rs", "upstream/core/src/ddi_snapshot.rs"),
    ] {
        if artifact(&verified, dest)? != reviewed[name] {
            return Err(invalid("Prepared hook source differs"));
        }
    }
    let template = std::str::from_utf8(reviewed["Cargo.toml.in"])
        .map_err(|_| invalid("Template UTF8"))?
        .replace("@UPSTREAM@", "../upstream");
    if artifact(&verified, "worker/Cargo.toml")? != template.as_bytes() {
        return Err(invalid("Prepared Cargo manifest differs"));
    }
    for (path, hook) in HOOKS {
        let bytes = artifact(&verified, &format!("upstream/{path}"))?;
        let original = bytes
            .strip_suffix(hook.as_bytes())
            .ok_or_else(|| invalid("Prepared hook append differs"))?;
        if source["upstream"]["files"][path]["sha256"] != sha256(original) {
            return Err(invalid("Prepared hook original differs"));
        }
    }
    let identity = artifact(&verified, "worker/build-identity.json")?.to_vec();
    if json::<Value>(&identity)? != supervisor::expected_build_identity() {
        return Err(invalid("Source build identity differs"));
    }
    native::external(&executable.canonicalize().map_err(native::io)?)?;
    let binary = bounded(executable, WORKER_LIMIT)?;
    if !valid_hash(executable_sha256) || sha256(&binary) != executable_sha256 {
        return Err(invalid("Built worker differs from explicit hash"));
    }
    platform_binary(&binary)?;
    let mut owned = output(destination)?;
    let mut inventory = Vec::new();
    for (path, b) in [
        ("ddi-ruffle-probe", binary.as_slice()),
        ("build-identity.json", identity.as_slice()),
        (
            "worker.sb",
            include_bytes!("../../ruffle/worker.sb").as_slice(),
        ),
    ] {
        checkpoint()?;
        inventory.push(write(&owned.path, path, b)?);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            owned.path.join("ddi-ruffle-probe"),
            fs::Permissions::from_mode(0o500),
        )
        .map_err(native::io)?;
    }
    for (from, to) in [
        ("upstream/LICENSE.md", "licenses/Ruffle-LICENSE.md"),
        ("licenses/DDI-MIT.txt", "licenses/DDI-MIT.txt"),
        ("worker/Cargo.lock", "provenance/Cargo.lock"),
    ] {
        inventory.push(write(&owned.path, to, artifact(&verified, from)?)?);
    }
    // Keep licences for assets embedded by the pinned Ruffle source too.
    for row in source["artifacts"].as_array().unwrap() {
        let path = row["path"].as_str().unwrap();
        let name = path.rsplit('/').next().unwrap().to_ascii_lowercase();
        if path.starts_with("upstream/")
            && path != "upstream/LICENSE.md"
            && (name == "license"
                || name.starts_with("license.")
                || name.starts_with("license-")
                || name == "licence"
                || name.starts_with("licence.")
                || name.starts_with("licence-")
                || name == "ofl.txt"
                || name == "notice")
        {
            inventory.push(write(
                &owned.path,
                &format!("licenses/{path}"),
                artifact(&verified, path)?,
            )?);
        }
    }
    inventory.push(write(
        &owned.path,
        "provenance/source-manifest.json",
        &source_bytes,
    )?);
    inventory.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    checkpoint()?;
    let receipt = json!({"schema":"ddi-danoni-worker-bundle/1","producer":"rust-worker-package/1","platform":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH},"build":supervisor::expected_build_identity(),"worker":{"executable":"ddi-ruffle-probe","sha256":executable_sha256,"buildIdentity":"build-identity.json","buildIdentitySha256":sha256(&identity)},"sandboxSha256":sha256(include_bytes!("../../ruffle/worker.sb")),"sourceManifestSha256":sha256(&source_bytes),"artifacts":inventory,"scope":"Operator-selected local worker bundle; hashes are integrity, not signatures or compiler attestation. Third-party Cargo dependencies retain their own licences; redistribution requires their notices too."});
    ready(&owned.path, &receipt)?;
    owned.ready = true;
    Ok(receipt)
}
/// Read a trusted installed bundle, verify every inventoried byte, and return the
/// same WorkerSpec consumed by the restricted supervisor. A song cannot supply it.
pub fn load_bundle(root: &Path, checkpoint: &mut impl FnMut() -> Result<()>) -> Result<WorkerSpec> {
    checkpoint()?;
    let root = root.canonicalize().map_err(native::io)?;
    let m = manifest(&root)?;
    if m["schema"] != "ddi-danoni-worker-bundle/1"
        || m["platform"]["os"] != std::env::consts::OS
        || m["platform"]["arch"] != std::env::consts::ARCH
        || m["build"] != supervisor::expected_build_identity()
        || m["sandboxSha256"] != sha256(include_bytes!("../../ruffle/worker.sb"))
    {
        return Err(invalid("Unreviewed bundle/platform identity"));
    }
    let verified = verify_inventory(&root, &m, WORKER_LIMIT, checkpoint)?;
    let w = &m["worker"];
    if w["executable"] != "ddi-ruffle-probe" || w["buildIdentity"] != "build-identity.json" {
        return Err(invalid("Unexpected bundle worker paths"));
    }
    let binary = artifact(&verified, "ddi-ruffle-probe")?;
    platform_binary(binary)?;
    let identity = artifact(&verified, "build-identity.json")?;
    if w["sha256"] != sha256(binary)
        || w["buildIdentitySha256"] != sha256(identity)
        || json::<Value>(identity)? != supervisor::expected_build_identity()
        || artifact(&verified, "worker.sb")? != include_bytes!("../../ruffle/worker.sb")
    {
        return Err(invalid("Bundle runtime differs"));
    }
    let provenance = artifact(&verified, "provenance/source-manifest.json")?;
    if m["sourceManifestSha256"] != sha256(provenance) {
        return Err(invalid("Bundle source provenance differs"));
    }
    checkpoint()?;
    Ok(WorkerSpec {
        executable: root.join("ddi-ruffle-probe"),
        sha256: sha256(binary),
        build_identity: root.join("build-identity.json"),
        build_identity_sha256: sha256(identity),
    })
}
pub fn package_job(
    job: &Path,
    destination: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    let path = job.canonicalize().map_err(native::io)?;
    native::external(&path)?;
    let job: PackageJob = json(&bounded(&path, 65536)?)?;
    if job.schema != "ddi-danoni-worker-package-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let resolve = |p: PathBuf| {
        if p.is_absolute() {
            p
        } else {
            path.parent().unwrap().join(p)
        }
    };
    package_worker(
        &resolve(job.sources),
        &resolve(job.executable),
        &job.executable_sha256,
        destination,
        checkpoint,
    )
}
