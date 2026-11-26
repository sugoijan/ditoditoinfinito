//! The plugins installed in the data folder's `plugins/` (`docs/plans/
//! source-plugins.md`, "Installing"): each a folder `<id>/` with its
//! manifest, `plugin.toml`, and its executable for this platform,
//! `bin/<arch>-<os>/<executable>` (`.exe` added on Windows), whose SHA-256
//! is shown for comparing with what its publisher lists. Reading them
//! starts nothing.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use ddi_source_protocol::Manifest;
use ddi_source_protocol::limits::MAX_MANIFEST_BYTES;
use sha2::Digest as _;

/// The manifest's file name.
pub const MANIFEST: &str = "plugin.toml";

/// Plugin folders looked at, at most (in name order).
pub const MAX_PLUGINS: usize = 256;

/// A plugin found ready to start.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    /// Its folder, `plugins/<id>/`.
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// Its executable for this platform.
    pub executable: PathBuf,
    /// The executable's SHA-256, in lowercase hex.
    pub sha256: String,
}

impl Installed {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }
}

/// Why a folder in `plugins/` is not a plugin the game can start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// No `plugin.toml`, or it could not be read (the reason, data).
    NoManifest(String),
    /// Its manifest is not one the game takes (why, data).
    Manifest(String),
    /// Its manifest's id is not its folder's name.
    NotItsFolder,
    /// No executable for this platform where the manifest says (where
    /// it was looked for).
    NoExecutable(PathBuf),
}

/// A folder found and what it is.
#[allow(clippy::large_enum_variant)] // A few, listed once.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    Plugin(Installed),
    Refused { dir: PathBuf, problem: Problem },
}

/// The folder in `bin/` this platform's executables are in:
/// `<arch>-<os>` as Rust names them (`aarch64-macos`, `x86_64-linux`,
/// `x86_64-windows`).
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

/// The plugins in `plugins_dir`, in their folders' name order, at most
/// [`MAX_PLUGINS`]: those ready to start and those refused, with why. A
/// missing folder is no plugins. Hashes each executable: run it off the
/// frame.
pub fn discover(plugins_dir: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.truncate(MAX_PLUGINS);
    dirs.into_iter()
        .map(|dir| match read(&dir) {
            Ok(p) => Found::Plugin(p),
            Err(problem) => Found::Refused { dir, problem },
        })
        .collect()
}

/// The plugin in `dir`.
pub fn read(dir: &Path) -> Result<Installed, Problem> {
    let manifest = read_manifest(&dir.join(MANIFEST))?;
    if dir.file_name().and_then(|n| n.to_str()) != Some(manifest.id.as_str()) {
        return Err(Problem::NotItsFolder);
    }
    let executable = dir
        .join("bin")
        .join(platform())
        .join(format!("{}{}", manifest.executable, std::env::consts::EXE_SUFFIX));
    if !executable.is_file() {
        return Err(Problem::NoExecutable(executable));
    }
    let sha256 = sha256(&executable).map_err(|_| Problem::NoExecutable(executable.clone()))?;
    Ok(Installed {
        dir: dir.to_path_buf(),
        manifest,
        executable,
        sha256,
    })
}

/// A manifest file, read whole up to [`MAX_MANIFEST_BYTES`] and checked.
pub fn read_manifest(path: &Path) -> Result<Manifest, Problem> {
    let file = std::fs::File::open(path).map_err(|e| Problem::NoManifest(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Problem::NoManifest(e.to_string()))?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(Problem::Manifest(format!("over {MAX_MANIFEST_BYTES} bytes")));
    }
    let text = String::from_utf8(bytes).map_err(|_| Problem::Manifest("not UTF-8".into()))?;
    parse_manifest(&text)
}

/// A manifest's text, parsed and checked.
pub fn parse_manifest(text: &str) -> Result<Manifest, Problem> {
    let manifest: Manifest = toml::from_str(text).map_err(|e| Problem::Manifest(e.to_string()))?;
    match manifest.problem() {
        Some(p) => Err(Problem::Manifest(p)),
        None => Ok(manifest),
    }
}

/// A file's SHA-256, in lowercase hex.
pub fn sha256(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 64 << 10];
    loop {
        match file.read(&mut buf)? {
            0 => break,
            n => hasher.update(&buf[..n]),
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST_TEXT: &str = r#"
id = "example"
name = { en = "Example", ja = "例" }
publisher = "Tests"
version = "1.0.0"
protocol = [1]
executable = "example"
capabilities = ["search", "fetch"]
"#;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ddi-source-installed-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn plugins_are_found_with_their_digest_and_refusals_say_why() {
        let root = scratch("discover");
        let good = root.join("example");
        let bin = good.join("bin").join(platform());
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(good.join(MANIFEST), MANIFEST_TEXT).unwrap();
        std::fs::write(
            bin.join(format!("example{}", std::env::consts::EXE_SUFFIX)),
            b"abc",
        )
        .unwrap();
        let misnamed = root.join("other");
        std::fs::create_dir_all(&misnamed).unwrap();
        std::fs::write(misnamed.join(MANIFEST), MANIFEST_TEXT).unwrap();
        let empty = root.join("zzz");
        std::fs::create_dir_all(&empty).unwrap();
        let found = discover(&root);
        assert_eq!(found.len(), 3);
        let Found::Plugin(p) = &found[0] else {
            panic!("{found:?}")
        };
        assert_eq!(p.id(), "example");
        // SHA-256 of "abc".
        assert_eq!(
            p.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(matches!(
            found[1],
            Found::Refused {
                problem: Problem::NotItsFolder,
                ..
            }
        ));
        assert!(matches!(
            found[2],
            Found::Refused {
                problem: Problem::NoManifest(_),
                ..
            }
        ));
        assert!(discover(&root.join("missing")).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifests_with_unknown_keys_or_past_the_limits_are_refused() {
        assert!(parse_manifest(MANIFEST_TEXT).is_ok());
        let unknown = format!("{MANIFEST_TEXT}\nauto_update = true\n");
        assert!(matches!(parse_manifest(&unknown), Err(Problem::Manifest(_))));
        let root = scratch("huge");
        let path = root.join(MANIFEST);
        std::fs::write(&path, " ".repeat(MAX_MANIFEST_BYTES + 1)).unwrap();
        assert!(matches!(read_manifest(&path), Err(Problem::Manifest(_))));
        std::fs::remove_dir_all(root).unwrap();
    }
}
