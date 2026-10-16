//! Host-selected, relocatable native installation and URL-only defaults.
//! Installation hashes establish integrity, not publisher authenticity. No shell
//! command or development helper is invoked by installation, diagnosis or import.
use crate::{
    Error, Result, artwork,
    assets::*,
    import_job,
    managed_store::Store,
    native,
    network::NetworkCancel,
    supervisor::{self, WorkerSpec},
    url_native::RendererPolicy,
    worker_build,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const MAX: usize = 256 * 1024 * 1024;
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-native-runtime", message)
}
fn identity() -> Value {
    json!({"contract":"ddi-danoni-installed-runtime/1", "sdkVersion":env!("CARGO_PKG_VERSION"), "vorbis":cfg!(feature="vorbis"),
        "sdkLockSha256":sha256(include_bytes!("../Cargo.lock")),
        "hostImplementationSha256":sha256(include_bytes!("runtime.rs")),
        "artworkImplementationSha256":sha256(include_bytes!("artwork.rs")),
        "preparationImplementationSha256":sha256(include_bytes!("preparation.rs")),
        "urlImplementationSha256":sha256(include_bytes!("url_native/mod.rs")),
        "worker":supervisor::expected_build_identity(), "fonts":artwork::portable_fonts(), "optionalColorReviewsSha256":sha256(include_bytes!("../reviews/optional-colors.json"))})
}
fn read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let m = fs::symlink_metadata(path).map_err(native::io)?;
    if !m.is_file() || m.len() > maximum as u64 {
        return Err(invalid("Expected bounded regular file, without symlinks"));
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(native::io)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(native::io)?;
    if bytes.len() > maximum {
        return Err(invalid("Runtime file exceeds limit"));
    }
    Ok(bytes)
}
fn tree(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn walk(
        root: &Path,
        dir: &Path,
        files: &mut BTreeMap<String, Vec<u8>>,
        total: &mut usize,
    ) -> Result<()> {
        for entry in fs::read_dir(dir).map_err(native::io)? {
            let path = entry.map_err(native::io)?.path();
            let meta = fs::symlink_metadata(&path).map_err(native::io)?;
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .ok_or_else(|| invalid("Non UTF-8 runtime filename"))?
                .replace('\\', "/");
            relative_path(&name)?;
            if meta.file_type().is_symlink() {
                return Err(invalid("Runtime symlink"));
            }
            if meta.is_dir() {
                walk(root, &path, files, total)?;
            } else {
                let bytes = read(&path, MAX)?;
                *total = total
                    .checked_add(bytes.len())
                    .ok_or_else(|| invalid("Runtime size overflow"))?;
                if *total > 512 * 1024 * 1024 || files.len() >= 10000 {
                    return Err(invalid("Runtime inventory exceeds limits"));
                }
                files.insert(name, bytes);
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files, &mut 0)?;
    Ok(files)
}
fn inventory(files: &BTreeMap<String, Vec<u8>>) -> Vec<Value> {
    files
        .iter()
        .map(|(p, b)| json!({"path":p,"bytes":b.len(),"sha256":sha256(b)}))
        .collect()
}
fn write(root: &Path, path: &str, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    relative_path(path)?;
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).map_err(native::io)?;
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&p)
        .map_err(native::io)?;
    f.write_all(bytes).map_err(native::io)?;
    f.sync_all().map_err(native::io)
}
fn validate_notices(files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let data = files
        .get("notices/manifest.json")
        .ok_or_else(|| invalid("Missing notices manifest"))?;
    let notice: Value = crate::assets::json(data)?;
    let locks = notice["locks"]
        .as_array()
        .ok_or_else(|| invalid("Missing notices lock bindings"))?;
    for (component, path) in [
        ("sdk", "notices/locks/sdk-Cargo.lock"),
        ("worker", "notices/locks/worker-Cargo.lock"),
    ] {
        let bytes = files
            .get(path)
            .ok_or_else(|| invalid("Missing noticed lockfile"))?;
        if !locks
            .iter()
            .any(|r| r["component"] == component && r["sha256"] == sha256(bytes))
        {
            return Err(invalid("Notices lock binding differs"));
        }
    }
    if notice["schema"] != "ddi-danoni-runtime-notices/1"
        || files.get("notices/locks/sdk-Cargo.lock").map(Vec::as_slice)
            != Some(include_bytes!("../Cargo.lock").as_slice())
        || files.get("notices/locks/worker-Cargo.lock") != files.get("worker/provenance/Cargo.lock")
    {
        return Err(invalid(
            "Dependency notices must be collected for these exact SDK/worker lockfiles",
        ));
    }
    let rows = notice["artifacts"]
        .as_array()
        .ok_or_else(|| invalid("Missing notices inventory"))?;
    let mut expected = BTreeMap::new();
    for row in rows {
        let name = row["path"]
            .as_str()
            .ok_or_else(|| invalid("Notices filename"))?;
        relative_path(name)?;
        if expected.insert(format!("notices/{name}"), row).is_some() {
            return Err(invalid("Duplicate notice"));
        }
    }
    for (name, bytes) in files
        .iter()
        .filter(|(p, _)| p.starts_with("notices/") && p.as_str() != "notices/manifest.json")
    {
        let row = expected
            .remove(name)
            .ok_or_else(|| invalid("Uninventoried notice"))?;
        if row["bytes"].as_u64() != Some(bytes.len() as u64) || row["sha256"] != sha256(bytes) {
            return Err(invalid("Changed collected notice"));
        }
    }
    if !expected.is_empty() {
        return Err(invalid("Missing collected notice"));
    }
    Ok(())
}
/// Copy an already reviewed bundle and this SDK executable into a fresh prefix.
/// `notices` is a build-time collected directory, never provided by a work URL.
/// No overwrites; a failed installation has no manifest and is not loadable.
pub fn install(bundle: &Path, executable: &Path, notices: &Path, output: &Path) -> Result<Value> {
    if !cfg!(feature = "vorbis") {
        return Err(invalid(
            "Build the installed importer with native and vorbis features",
        ));
    }
    worker_build::load_bundle(bundle, &mut || Ok(()))?;
    let mut files = BTreeMap::new();
    for (p, b) in tree(bundle)? {
        files.insert(format!("worker/{p}"), b);
    }
    let executable_bytes = read(executable, MAX)?;
    worker_build::platform_binary(&executable_bytes)?;
    files.insert("bin/ddi-danoni-export".into(), executable_bytes);
    let notice_files = tree(notices)?;
    if !notice_files.contains_key("manifest.json") {
        return Err(invalid(
            "Run the documented maintainer notice collector first",
        ));
    }
    for (p, b) in notice_files {
        files.insert(format!("notices/{p}"), b);
    }
    for (name, bytes) in [
        (
            "fonts/IosevkaTerm-Regular-latin.ttf",
            artwork::PORTABLE_LATIN,
        ),
        (
            "fonts/NotoSansCJKjp-Regular.otf",
            artwork::PORTABLE_JAPANESE,
        ),
        (
            "licenses/Iosevka-OFL.txt",
            include_bytes!("../fonts/Iosevka-LICENSE.txt").as_slice(),
        ),
        (
            "licenses/Noto-OFL.txt",
            include_bytes!("../fonts/Noto-LICENSE.txt").as_slice(),
        ),
        (
            "reviews/optional-colors.json",
            include_bytes!("../reviews/optional-colors.json").as_slice(),
        ),
        (
            "fonts/PROVENANCE.toml",
            include_bytes!("../fonts/PROVENANCE.toml").as_slice(),
        ),
        (
            "licenses/DDI-MIT.txt",
            include_bytes!("../../../../LICENSES/MIT.txt").as_slice(),
        ),
    ] {
        files.insert(name.into(), bytes.to_vec());
    }
    let parent = output
        .parent()
        .ok_or_else(|| invalid("Installation parent"))?
        .canonicalize()
        .map_err(native::io)?;
    native::external(&parent)?;
    let mut owned = crate::orchestration::new_root(output)?;
    let output = owned.root.as_path();
    validate_notices(&files)?;
    for (p, b) in &files {
        write(output, p, b)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in ["bin/ddi-danoni-export", "worker/ddi-ruffle-probe"] {
            fs::set_permissions(output.join(p), fs::Permissions::from_mode(0o500))
                .map_err(native::io)?;
        }
    }
    worker_build::load_bundle(&output.join("worker"), &mut || Ok(()))?;
    let manifest = json!({"schema":"ddi-danoni-installed-runtime/1","identity":identity(),
        "platform":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH},
        "artifacts":inventory(&files),
        "systemPrerequisites":"Supported confinement kernel, host ELF libraries; Linux Vulkan loader and CPU Mesa ICD for rendering. No graphics libraries are shipped.",
        "distributionScope":"Local installation; review notices/manifest.json before redistribution. Hashes are not signatures."});
    write(
        output,
        "manifest.json",
        &serde_json::to_vec_pretty(&manifest).map_err(|e| invalid(e.to_string()))?,
    )?;
    owned.retain = true;
    Ok(manifest)
}
/// A verified host installation. Never obtain the location from acquired content.
pub struct Installation {
    pub root: PathBuf,
    pub worker: WorkerSpec,
    pub manifest: Value,
}
impl Installation {
    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize().map_err(native::io)?;
        let raw = read(&root.join("manifest.json"), JSON_LIMIT)?;
        let manifest: Value = crate::assets::json(&raw)?;
        if manifest["schema"] != "ddi-danoni-installed-runtime/1"
            || manifest["identity"] != identity()
            || manifest["platform"]["os"] != std::env::consts::OS
            || manifest["platform"]["arch"] != std::env::consts::ARCH
        {
            return Err(invalid(
                "Runtime version/platform differs; install the matching SDK and worker together",
            ));
        }
        let mut files = tree(&root)?;
        files.remove("manifest.json");
        if manifest["artifacts"] != json!(inventory(&files)) {
            return Err(invalid(
                "Runtime inventory/hash differs; reinstall into a fresh directory",
            ));
        }
        for required in [
            "bin/ddi-danoni-export",
            "notices/manifest.json",
            "licenses/DDI-MIT.txt",
            "licenses/Noto-OFL.txt",
            "licenses/Iosevka-OFL.txt",
            "fonts/PROVENANCE.toml",
        ] {
            if !files.contains_key(required) {
                return Err(invalid("Incomplete runtime inventory"));
            }
        }
        for (p, b) in [
            (
                "fonts/IosevkaTerm-Regular-latin.ttf",
                artwork::PORTABLE_LATIN,
            ),
            (
                "fonts/NotoSansCJKjp-Regular.otf",
                artwork::PORTABLE_JAPANESE,
            ),
        ] {
            if files.get(p).map(Vec::as_slice) != Some(b) {
                return Err(invalid("Packaged font differs from the compiled renderer"));
            }
        }
        worker_build::platform_binary(&files["bin/ddi-danoni-export"])?;
        if files.get("reviews/optional-colors.json").map(Vec::as_slice)
            != Some(include_bytes!("../reviews/optional-colors.json").as_slice())
        {
            return Err(invalid(
                "Packaged optional-colour reviews differ from compiled host defaults",
            ));
        }
        validate_notices(&files)?;
        let worker = worker_build::load_bundle(&root.join("worker"), &mut || Ok(()))?;
        Ok(Self {
            root,
            worker,
            manifest,
        })
    }
    /// Host-selected path, explicit environment, then executable's sibling prefix.
    pub fn locate(explicit: Option<&Path>) -> Result<Self> {
        if let Some(path) = explicit {
            return Self::load(path);
        }
        if let Some(path) = std::env::var_os("DDI_DANONI_RUNTIME") {
            return Self::load(Path::new(&path));
        }
        let executable = std::env::current_exe().map_err(native::io)?;
        Self::load(
            executable
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| invalid("Set DDI_DANONI_RUNTIME to the installed prefix"))?,
        )
    }
    /// Shared defaults for CLI and embedding hosts; no per-work recipe or flags.
    pub fn request(&self, url: impl Into<String>, offline: bool) -> import_job::Request {
        let mut request = import_job::Request::new(url);
        request.preparation.preparation.portable_artwork = true;
        request.preparation.source.renderer_policy = RendererPolicy::ReviewedNativeScale;
        request.preparation.source.fetch.offline = offline;
        request.preparation.source.optional_color_reviews =
            serde_json::from_slice(include_bytes!("../reviews/optional-colors.json"))
                .expect("compiled reviewed colour bindings");
        request
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostConfig {
    pub runtime: Option<PathBuf>,
    pub cache: PathBuf,
    pub staging: PathBuf,
    pub library: PathBuf,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub origins: BTreeMap<String, String>,
}
impl HostConfig {
    pub fn read(path: &Path) -> Result<Self> {
        let path = path.canonicalize().map_err(native::io)?;
        let mut config: Self = crate::assets::json(&read(&path, JSON_LIMIT)?)?;
        let parent = path.parent().unwrap();
        for p in [&mut config.cache, &mut config.staging, &mut config.library]
            .into_iter()
            .chain(config.runtime.iter_mut())
        {
            if !p.is_absolute() {
                *p = parent.join(&*p);
            }
        }
        Ok(config)
    }
}
/// A host can retain this configuration and reuse `import_job::execute` directly.
/// This CLI convenience adapter owns no UI, hidden queue or permissive site policy.
pub fn import_url(config: &HostConfig, url: &str) -> Result<import_job::Outcome> {
    let install = Installation::locate(config.runtime.as_deref())?;
    let staging = config.staging.canonicalize().map_err(native::io)?;
    native::external(&staging)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| invalid(e.to_string()))?
        .as_nanos();
    let output = staging.join(format!("import-{nonce}-{}", std::process::id()));
    let assets = native::DirectorySource::new(BTreeMap::new())?;
    let mut store = Store::open(&config.library)?;
    import_job::execute(
        &install.request(url, config.offline),
        &import_job::Services {
            cache: &config.cache,
            worker: &install.worker,
            assets: &assets,
        },
        &output,
        &mut store,
        &NetworkCancel::default(),
        &mut |url| {
            let origin = url::Url::parse(url)
                .map_err(|e| invalid(e.to_string()))?
                .origin()
                .ascii_serialization();
            config.origins.get(&origin).filter(|s|!s.trim().is_empty()).cloned().ok_or_else(||Error::new("site-terms-denied",format!("Review public-access terms for {origin} and add its decision to host origins; robots rules still apply")))
        },
        &mut |_| Ok(()),
    )
}

/// Execute the packaged original bitmap fixture only inside production
/// confinement. Proves adapter selection, readback AND ActionScript pixel reads.
/// A failure retains captures and diagnostic.json; it never retries unconfined.
pub fn diagnose(installation: &Installation, output: &Path) -> Result<Value> {
    use crate::supervisor::{CaptureRequest, Limits};
    struct Memory(BTreeMap<String, Vec<u8>>);
    impl AssetSource for Memory {
        fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
            self.0
                .get(path)
                .filter(|b| ns == "probe" && b.len() <= max)
                .cloned()
                .ok_or_else(|| invalid("Unknown capability fixture input"))
        }
    }
    let mut owned = crate::orchestration::new_root(output)?;
    owned.retain = true;
    let result = (|| {
        let bitmap = include_bytes!("../fixtures/runtime-bitmap.swf").to_vec();
        let renderer = RendererPolicy::ReviewedNativeScale
            .renderer(crate::profiles::EngineProfile::RekuScheduledMenu)
            .unwrap();
        let config=serde_json::to_vec(&json!({"movie":"bitmap.swf","sourceUrl":"https://example.invalid/bitmap.swf", "captureFrames":[1,2],"render":true,"renderScale":1,"renderer":renderer,"resources":[]})).map_err(|e|invalid(e.to_string()))?;
        let bind = |name: &str, b: &[u8]| Binding {
            namespace: "probe".into(),
            path: name.into(),
            sha256: sha256(b),
        };
        let request = CaptureRequest {
            config: bind("config.json", &config),
            movie: bind("bitmap.swf", &bitmap),
            provenance: None,
            limits: Limits::default(),
        };
        let source = Memory(BTreeMap::from([
            ("config.json".into(), config),
            ("bitmap.swf".into(), bitmap),
        ]));
        let capture = supervisor::capture(
            &request,
            &installation.worker,
            &source,
            &owned.root.join("bitmap"),
            &mut || Ok(()),
        )?;
        if !capture.ready {
            return Err(capture
                .failure
                .unwrap_or_else(|| invalid("Capability capture was not Ready")));
        }
        let snapshot: Value = crate::assets::json(&read(
            &owned.root.join("bitmap/capture/snapshot.json"),
            JSON_LIMIT,
        )?)?;
        renderer
            .validate_report(&snapshot["renderer"])
            .map_err(invalid)?;
        if snapshot["audit"]["runtimeErrors"] != 0 || snapshot["avm1Halted"] != false {
            return Err(invalid("Bitmap runtime audit failed"));
        }
        for (frame, rgb) in [(1, [200u8, 80, 40]), (2, [40u8, 160, 80])] {
            let state = &snapshot["captures"][frame - 1]["state"];
            let roots = state["roots"]
                .as_array()
                .ok_or_else(|| invalid("Missing probe roots"))?;
            let root = roots
                .iter()
                .find(|r| r["level"] == 0)
                .ok_or_else(|| invalid("Missing probe root"))?;
            let index = root["value"]["ref"]
                .as_u64()
                .ok_or_else(|| invalid("Missing probe object"))? as usize;
            let properties = state["objects"][index]["properties"]
                .as_array()
                .ok_or_else(|| invalid("Missing probe properties"))?;
            let color = properties
                .iter()
                .find(|p| p["name"]["text"] == "bitmapColor")
                .and_then(|p| p["value"].as_f64());
            if color != Some(u32::from_be_bytes([0, rgb[0], rgb[1], rgb[2]]) as f64) {
                return Err(invalid(
                    "ActionScript BitmapData.draw/getPixel result differs",
                ));
            }
            let ppm = read(
                &owned.root.join(format!("bitmap/capture/frame-{frame}.ppm")),
                1024 * 1024,
            )?;
            let header = b"P6\n320 240\n255\n";
            let at = header.len() + (50 * 320 + 50) * 3;
            if !ppm.starts_with(header)
                || ppm.len() != header.len() + 320 * 240 * 3
                || ppm[header.len()..header.len() + 3] != [17, 34, 51]
                || ppm[at..at + 3] != rgb
            {
                return Err(invalid("Native-scale frame readback differs"));
            }
        }
        artwork::portable_title_cards(
            "上海紅茶館 ～ Chinese Tea",
            "上海アリス幻樂団 / ZUN",
            &"0".repeat(64),
            vec![artwork::Role::Banner],
        )?;
        Ok(
            json!({"renderer":snapshot["renderer"],"audit":snapshot["audit"],"bitmapFrames":2,"readback":true,"actionScriptPixelReads":true,"portableJapaneseTitle":true}),
        )
    })();
    let diagnostic = json!({"schema":"ddi-danoni-runtime-diagnostic/1","passed":result.is_ok(),"runtime":installation.root,"identity":identity(),"checks":result.as_ref().ok(),"failure":result.as_ref().err(),
        "remedy":"Use x86-64 Linux with Landlock ABI >=6 and permitted seccomp/close_range. Install host ELF libraries, Vulkan loader and Mesa Lavapipe CPU ICD in distribution paths. Inspect bitmap/capture/stderr.txt. Linux renderScale >1 is unsupported; no confinement fallback.","scope":"Original synthetic confined test; not real-chart equivalence or self-contained distribution"});
    write(
        &owned.root,
        "diagnostic.json",
        &serde_json::to_vec_pretty(&diagnostic).map_err(|e| invalid(e.to_string()))?,
    )?;
    match result {
        Ok(_) => Ok(diagnostic),
        Err(e) => Err(Error::new(
            "native-runtime-capability",
            format!(
                "{e}; see {}. Install system Vulkan loader and Mesa CPU driver for rendering; kernel confinement must also be supported.",
                owned.root.join("diagnostic.json").display()
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_defaults_keep_all_budgets_and_select_only_reviewed_rendering_families() {
        let install = Installation {
            root: PathBuf::new(),
            worker: WorkerSpec {
                executable: PathBuf::new(),
                sha256: String::new(),
                build_identity: PathBuf::new(),
                build_identity_sha256: String::new(),
            },
            manifest: Value::Null,
        };
        let request = install.request("https://example.invalid/work.html", true);
        let standard = import_job::Request::new("https://example.invalid/work.html");
        assert!(request.preparation.preparation.portable_artwork);
        assert!(request.preparation.font.is_none());
        assert!(request.preparation.source.renderer.is_none());
        assert_eq!(request.preparation.source.optional_color_reviews.len(), 5);
        for review in &request.preparation.source.optional_color_reviews {
            assert_eq!(review.source_swf_sha256.len(), 64);
            assert_eq!(review.response_sha256.len(), 64);
            assert!(review.url.ends_with("/iro.txt"));
        }
        assert!(request.preparation.source.fetch.offline);
        assert_eq!(request.wall_ms, standard.wall_ms);
        assert_eq!(request.preparation.wall_ms, standard.preparation.wall_ms);
        assert_eq!(
            request.preparation.source.profile.wall_ms,
            standard.preparation.source.profile.wall_ms
        );
        assert_eq!(
            serde_json::to_value(request.preparation.source.capture).unwrap(),
            serde_json::to_value(standard.preparation.source.capture).unwrap()
        );
        use crate::profiles::EngineProfile::*;
        assert!(
            RendererPolicy::ReviewedNativeScale
                .renderer(RekuConstantMenu)
                .is_none()
        );
        for profile in [RekuScheduledMenu, RekuConstantMenuEarly] {
            assert!(RendererPolicy::ExplicitOnly.renderer(profile).is_none());
            let renderer = RendererPolicy::ReviewedNativeScale
                .renderer(profile)
                .unwrap();
            #[cfg(target_os = "linux")]
            {
                assert_eq!(
                    renderer.backend,
                    supervisor::renderer_options::Backend::Vulkan
                );
                assert_eq!(
                    renderer.adapter,
                    supervisor::renderer_options::Adapter::Software
                );
            }
            assert!(renderer.adapter_name.is_none());
        }
    }
    #[test]
    fn host_config_rejects_unknown_options_and_bounded_reader_refuses_symlinks() {
        assert!(serde_json::from_value::<HostConfig>(json!({"runtime":null,"cache":"cache","staging":"staging","library":"library","relaxSandbox":true})).is_err());
        let root = std::env::temp_dir().join(format!("ddi-runtime-reader-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("file"), b"abc").unwrap();
        assert!(read(&root.join("file"), 2).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("file"), root.join("link")).unwrap();
            assert!(read(&root.join("link"), 10).is_err());
            assert!(tree(&root).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
