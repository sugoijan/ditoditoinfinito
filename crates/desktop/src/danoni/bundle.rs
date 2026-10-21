//! Executable-relative, content-bound importer resources. No operator paths or
//! hashes are supplied at runtime; the application carries its bundle identity.
use ddi_library::path::RelPath;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) const ID: &str = env!("DDI_DANONI_BUNDLE_ID");
pub(super) const MANIFEST_SHA256: &str = env!("DDI_DANONI_BUNDLE_MANIFEST_SHA256");
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FILES: usize = 10000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema: String,
    id: String,
    protocol_version: u32,
    executable: String,
    artifacts: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Artifact {
    path: String,
    bytes: u64,
    sha256: String,
}

pub(super) fn locate() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Beside the executable, or in a macOS app's resources.
    let candidates: Vec<_> = crate::paths::package_dirs(&exe)
        .into_iter()
        .map(|d| d.join("danoni-importer"))
        .collect();
    let base = candidates
        .iter()
        .find(|p| fs::symlink_metadata(p).is_ok())
        .or(candidates.first())
        .ok_or("Cannot locate application directory")?;
    directory(base)?;
    let root = base.join(ID);
    verify(&root, ID, MANIFEST_SHA256)
        .map_err(|e| format!("The packaged DanOni importer is missing or damaged: {e}. Reinstall the complete application package."))
}
fn directory(path: &Path) -> Result<(), String> {
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .is_dir()
    {
        return Err("Expected a regular bundle directory without symlinks".into());
    }
    Ok(())
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}
fn digest(path: &Path, expected: u64) -> Result<String, String> {
    if expected > MAX_BYTES {
        return Err("Importer artifact exceeds size limit".into());
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() != expected {
        return Err("Importer artifact size/type differs".into());
    }
    let mut file = fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(expected + 1);
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut count = 0;
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        hash.update(&buffer[..n]);
    }
    if count != expected {
        return Err("Importer artifact changed while reading".into());
    }
    Ok(hex(hash.finalize()))
}
fn walk(root: &Path, path: &Path, files: &mut BTreeMap<String, PathBuf>) -> Result<(), String> {
    if path
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .components()
        .count()
        > 64
    {
        return Err("Importer bundle nesting exceeds limit".into());
    }
    directory(path)?;
    for item in fs::read_dir(path).map_err(|e| e.to_string())? {
        let path = item.map_err(|e| e.to_string())?.path();
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            walk(root, &path, files)?;
        } else if metadata.is_file() {
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Non-UTF-8 importer filename")?
                .to_owned();
            if name.contains('\\') {
                return Err("Backslashes are not valid importer filenames".into());
            }
            if name != "manifest.json" && files.insert(name, path).is_some() {
                return Err("Repeated importer filename".into());
            }
            if files.len() > MAX_FILES {
                return Err("Importer inventory exceeds file limit".into());
            }
        } else {
            return Err("Importer bundle contains a symlink or special file".into());
        }
    }
    Ok(())
}
fn verify(root: &Path, id: &str, expected_manifest: &str) -> Result<PathBuf, String> {
    directory(root)?;
    let path = root.join("manifest.json");
    let size = fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .len();
    if size > 4 * 1024 * 1024 || digest(&path, size)? != expected_manifest {
        return Err("Importer manifest differs from this application build".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    if hex(Sha256::digest(&bytes)) != expected_manifest {
        return Err("Importer manifest changed while reading".into());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if manifest.schema != "ddi-danoni-app-bundle/1"
        || manifest.id != id
        || manifest.protocol_version != ddi_importer_protocol::VERSION
        || manifest.executable != "bin/ddi-danoni-importer"
    {
        return Err("Importer bundle identity/protocol differs".into());
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files)?;
    let mut total = 0_u64;
    let mut executable = None;
    for artifact in manifest.artifacts {
        let logical = RelPath::new(&artifact.path);
        if artifact.path.is_empty()
            || artifact.path.contains('\\')
            || logical.components().any(|s| matches!(s, "." | ".."))
        {
            return Err("Invalid importer artifact path".into());
        }
        let path = logical
            .to_path(root)
            .ok_or("Invalid importer artifact path")?;
        if files.remove(&artifact.path).as_ref() != Some(&path) {
            return Err("Importer artifact missing or repeated".into());
        }
        total = total
            .checked_add(artifact.bytes)
            .ok_or("Importer inventory overflow")?;
        if total > MAX_BYTES {
            return Err("Importer inventory exceeds size limit".into());
        }
        if digest(&path, artifact.bytes)? != artifact.sha256 {
            return Err("Importer artifact hash differs".into());
        }
        if artifact.path == manifest.executable {
            executable = Some(path);
        }
    }
    if !files.is_empty() {
        return Err("Unexpected importer bundle files".into());
    }
    executable.ok_or_else(|| "Missing importer executable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocated_bundle_rejects_corruption_extra_files_and_symlinks() {
        let root = std::env::temp_dir().join(format!(
            "ddi-bundle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("bin")).unwrap();
        let file = root.join("bin/ddi-danoni-importer");
        fs::write(&file, b"helper").unwrap();
        let bytes=serde_json::to_vec(&serde_json::json!({"schema":"ddi-danoni-app-bundle/1","id":"test",
            "protocolVersion":ddi_importer_protocol::VERSION,"executable":"bin/ddi-danoni-importer",
            "artifacts":[{"path":"bin/ddi-danoni-importer","bytes":6,"sha256":hex(Sha256::digest(b"helper"))}]})).unwrap();
        fs::write(root.join("manifest.json"), &bytes).unwrap();
        let expected = hex(Sha256::digest(&bytes));
        assert_eq!(verify(&root, "test", &expected).unwrap(), file);
        let moved = root.with_extension("moved");
        fs::rename(&root, &moved).unwrap();
        assert!(verify(&moved, "test", &expected).is_ok());
        fs::write(moved.join("extra"), b"unknown").unwrap();
        assert!(verify(&moved, "test", &expected).is_err());
        fs::remove_file(moved.join("extra")).unwrap();
        fs::write(moved.join("bin\\ddi-danoni-importer"), b"helper").unwrap();
        assert!(verify(&moved, "test", &expected).is_err());
        fs::remove_file(moved.join("bin\\ddi-danoni-importer")).unwrap();
        fs::write(moved.join("bin/ddi-danoni-importer"), b"broken").unwrap();
        assert!(verify(&moved, "test", &expected).is_err());
        fs::remove_file(moved.join("bin/ddi-danoni-importer")).unwrap();
        std::os::unix::fs::symlink("/does-not-exist", moved.join("bin/ddi-danoni-importer"))
            .unwrap();
        assert!(verify(&moved, "test", &expected).is_err());
        fs::remove_dir_all(moved).unwrap();
    }
}
