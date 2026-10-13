//! Optional filesystem adapter. Never executes work code or external tools.
use crate::{Error, Result, assets::*, work::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) fn io(error: std::io::Error) -> Error {
    Error::new("filesystem", error.to_string())
}
pub(crate) fn external(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        if parent.join(".git").exists() {
            return Err(Error::new(
                "checkout-output",
                "Third-party inputs/outputs must stay outside Git checkouts",
            ));
        }
    }
    Ok(())
}
/// Explicit immutable roots; no path guessing or fallback to unrestricted reads.
pub struct DirectorySource {
    roots: BTreeMap<String, PathBuf>,
}
impl DirectorySource {
    pub fn new(roots: BTreeMap<String, PathBuf>) -> Result<Self> {
        let roots = roots
            .into_iter()
            .map(|(key, path)| {
                let path = path.canonicalize().map_err(io)?;
                external(&path)?;
                if !path.is_dir() {
                    return Err(Error::new(
                        "invalid-root",
                        "Asset roots must be directories",
                    ));
                }
                Ok((key, path))
            })
            .collect::<Result<_>>()?;
        Ok(Self { roots })
    }
}
impl AssetSource for DirectorySource {
    fn read(&self, namespace: &str, path: &str, maximum: usize) -> Result<Vec<u8>> {
        relative_path(path)?;
        let root = self
            .roots
            .get(namespace)
            .ok_or_else(|| Error::new("unknown-namespace", namespace))?;
        let mut file = root.clone();
        for part in path.split('/') {
            file.push(part);
            if fs::symlink_metadata(&file)
                .map_err(io)?
                .file_type()
                .is_symlink()
            {
                return Err(Error::new(
                    "asset-symlink",
                    "Asset symlinks are not accepted",
                ));
            }
        }
        let file = file.canonicalize().map_err(io)?;
        if !file.starts_with(root) || !file.is_file() {
            return Err(Error::new(
                "unsafe-path",
                "Asset is not a contained regular file",
            ));
        }
        let mut bytes = Vec::new();
        fs::File::open(&file)
            .map_err(io)?
            .take(maximum as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() > maximum {
            return Err(Error::new("asset-limit", "Asset exceeds its size limit"));
        }
        Ok(bytes)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub work: WorkRequest,
}
pub fn load_job(path: &Path) -> Result<(OfflineJob, DirectorySource)> {
    let path = path.canonicalize().map_err(io)?;
    external(&path)?;
    let mut body = Vec::new();
    fs::File::open(&path)
        .map_err(io)?
        .take(JSON_LIMIT as u64 + 1)
        .read_to_end(&mut body)
        .map_err(io)?;
    if body.len() > JSON_LIMIT {
        return Err(Error::new("asset-limit", "Oversized job document"));
    }
    let job: OfflineJob = json(&body)?;
    if job.schema != "ddi-danoni-offline-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let roots = job
        .roots
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                if value.is_absolute() {
                    value.clone()
                } else {
                    path.parent().unwrap().join(value)
                },
            )
        })
        .collect();
    let source = DirectorySource::new(roots)?;
    Ok((job, source))
}
struct Incomplete {
    path: PathBuf,
    armed: bool,
}
impl Drop for Incomplete {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Write only to a new external directory; the atomic manifest rename is the
/// readiness marker. Existing library/export trees are never replaced here.
pub fn write_prepared(
    work: &PreparedWork,
    output: &Path,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<()> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(io)?;
    external(&parent)?;
    let name = output
        .file_name()
        .ok_or_else(|| Error::new("unsafe-output", "Missing output name"))?;
    if name == "." || name == ".." || name == ".git" {
        return Err(Error::new("unsafe-output", "Invalid output name"));
    }
    if work.manifest["schema"] != "ddi-danoni-prepared-work/1" {
        return Err(Error::new(
            "unsupported-schema",
            "Unexpected prepared-work result",
        ));
    }
    let inventory = work.manifest["artifacts"]
        .as_array()
        .ok_or_else(|| Error::new("invalid-inventory", "Missing artifact inventory"))?;
    if inventory.len() != work.files.len() {
        return Err(Error::new(
            "invalid-inventory",
            "Artifact inventory coverage differs",
        ));
    }
    let mut paths = std::collections::BTreeSet::new();
    for file in &work.files {
        relative_path(&file.path)?;
        let expected = inventory.iter().find(|r| r["path"] == file.path);
        if !paths.insert(&file.path)
            || expected.is_none_or(|r| {
                r["sha256"] != sha256(&file.bytes) || r["bytes"] != file.bytes.len()
            })
        {
            return Err(Error::new(
                "changed-artifact",
                "Prepared artifact differs from its inventory",
            ));
        }
    }
    let output = parent.join(name);
    fs::create_dir(&output).map_err(io)?;
    let mut guard = Incomplete {
        path: output.clone(),
        armed: true,
    };
    for (i, file) in work.files.iter().enumerate() {
        control(Progress {
            stage: Stage::Assemble,
            completed: i,
            total: work.files.len(),
        })?;
        relative_path(&file.path)?;
        if !["songs/", "references/", "provenance/"]
            .iter()
            .any(|p| file.path.starts_with(p))
        {
            return Err(Error::new(
                "unsafe-output",
                "Unexpected output artifact namespace",
            ));
        }
        let path = output.join(&file.path);
        fs::create_dir_all(path.parent().unwrap()).map_err(io)?;
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(io)?;
        target.write_all(&file.bytes).map_err(io)?;
    }
    control(Progress {
        stage: Stage::Assemble,
        completed: work.files.len(),
        total: work.files.len(),
    })?;
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join(".manifest.tmp"))
        .map_err(io)?;
    serde_json::to_writer_pretty(&mut target, &work.manifest)
        .map_err(|e| Error::new("output-json", e.to_string()))?;
    target.write_all(b"\n").map_err(io)?;
    target.sync_all().map_err(io)?;
    drop(target);
    fs::rename(output.join(".manifest.tmp"), output.join("manifest.json")).map_err(io)?;
    guard.armed = false;
    Ok(())
}

pub(crate) fn bounded_file(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > maximum {
        return Err(Error::new("asset-limit", "Oversized document"));
    }
    Ok(bytes)
}
fn registry_binding(
    row: &serde_json::Value,
    namespace: &str,
    root: &Path,
    roots: &mut BTreeMap<String, PathBuf>,
    source: &DirectorySource,
) -> Result<Binding> {
    let relative = text(row, "path")?;
    relative_path(relative)?;
    let raw = source.read("registry", relative, JSON_LIMIT)?;
    let hash = text(row, "sha256")?;
    if sha256(&raw) != hash {
        return Err(Error::new("changed-asset", "Registry binding changed"));
    }
    let path = root.join(relative);
    let parent = path.parent().unwrap().canonicalize().map_err(io)?;
    if !parent.starts_with(root) {
        return Err(Error::new("unsafe-path", "Registry binding escapes root"));
    }
    roots.insert(namespace.into(), parent);
    Ok(Binding {
        namespace: namespace.into(),
        path: path
            .file_name()
            .unwrap()
            .to_str()
            .ok_or_else(|| Error::new("unsafe-path", "Non-UTF-8 path"))?
            .into(),
        sha256: hash.into(),
    })
}

/// Migration bridge from reviewed source/reviews and existing accepted media.
/// No missing data is inferred, decoded or repaired by this bridge.
pub fn accepted_job(registry: &Path, exports: &Path, id: &str) -> Result<OfflineJob> {
    relative_path(id)?;
    if id.contains('/') {
        return Err(Error::new(
            "invalid-work-id",
            "Work id must be one path component",
        ));
    }
    let raw = bounded_file(registry, JSON_LIMIT)?;
    let registry: serde_json::Value = json(&raw)?;
    if registry["schema"] != "ddi-danoni-export-registry/1" {
        return Err(Error::new("unsupported-schema", "Unsupported registry"));
    }
    let root = PathBuf::from(text(&registry, "root")?)
        .canonicalize()
        .map_err(io)?;
    external(&root)?;
    let row = registry["works"]
        .as_array()
        .and_then(|ws| ws.iter().find(|w| w["id"] == id))
        .ok_or_else(|| Error::new("unknown-work", id))?;
    if row["required"] != true || row["kind"] != "normalized" {
        return Err(Error::new(
            "unsupported-work",
            "Bridge accepts required normalized entries only",
        ));
    }
    let prepared = exports
        .join("reports")
        .join(id)
        .canonicalize()
        .map_err(io)?;
    external(&prepared)?;
    let prepared_source =
        DirectorySource::new(BTreeMap::from([("accepted".into(), prepared.clone())]))?;
    let previous_body = prepared_source.read("accepted", "manifest.json", JSON_LIMIT)?;
    let previous: serde_json::Value = json(&previous_body)?;
    let original = DirectorySource::new(BTreeMap::from([("registry".into(), root.clone())]))?;
    let mut roots = BTreeMap::from([("accepted".into(), prepared.clone())]);
    let manifest = registry_binding(&row["source"], "source", &root, &mut roots, &original)?;
    if previous["sourceManifestSha256"] != manifest.sha256 {
        return Err(Error::new(
            "changed-source",
            "Accepted media uses another source manifest",
        ));
    }
    let mut reviews = ReviewBindings::default();
    if prepared.join("musical-grid-review.json").exists() {
        let raw = prepared_source.read("accepted", "musical-grid-review.json", JSON_LIMIT)?;
        reviews.grid = Some(Binding {
            namespace: "accepted".into(),
            path: "musical-grid-review.json".into(),
            sha256: sha256(&raw),
        });
    }
    for (key, namespace, target) in [
        ("nativeScroll", "motion", &mut reviews.motion),
        ("ratings", "ratings", &mut reviews.ratings),
        ("artwork", "artwork", &mut reviews.artwork),
    ] {
        if !row["policy"][key].is_null() {
            *target = Some(registry_binding(
                &row["policy"][key],
                namespace,
                &root,
                &mut roots,
                &original,
            )?);
        }
    }
    let provenance = Binding {
        namespace: "accepted".into(),
        path: "manifest.json".into(),
        sha256: sha256(&previous_body),
    };
    let mut media = Vec::new();
    for song in previous["songs"]
        .as_array()
        .ok_or_else(|| Error::new("invalid-document", "Missing accepted songs"))?
    {
        let audio_path = text(song, "music")?;
        let audio = prepared_source.read("accepted", audio_path, MEDIA_LIMIT)?;
        if sha256(&audio) != text(song, "musicSha256")? {
            return Err(Error::new("changed-asset", "Accepted music bytes changed"));
        }
        let pcm_path = format!("references/{}.wav", text(song, "id")?);
        let pcm = prepared_source.read("accepted", &pcm_path, MEDIA_LIMIT)?;
        media.push(SuppliedMedia {
            source_audio_sha256: text(song, "sourceAudioSha256")?.into(),
            audio: Binding {
                namespace: "accepted".into(),
                path: audio_path.into(),
                sha256: sha256(&audio),
            },
            pcm: Binding {
                namespace: "accepted".into(),
                path: pcm_path,
                sha256: sha256(&pcm),
            },
            codec: serde_json::from_value(previous["codec"].clone())
                .map_err(|e| Error::new("invalid-document", e.to_string()))?,
            provenance: provenance.clone(),
        });
    }
    let options = WorkOptions {
        tier_policy: serde_json::from_value(previous["tierPolicy"]["name"].clone())
            .map_err(|e| Error::new("invalid-document", e.to_string()))?,
        tier_bounds: serde_json::from_value(previous["tierPolicy"]["upperBounds"].clone())
            .map_err(|e| Error::new("invalid-document", e.to_string()))?,
        scroll_factor: previous["scrollCorrectionFactor"].as_f64().unwrap_or(1.0),
        display_bpms: if row["policy"]["displayBpms"].is_null() {
            BTreeMap::new()
        } else {
            serde_json::from_value(row["policy"]["displayBpms"].clone())
                .map_err(|e| Error::new("invalid-document", e.to_string()))?
        },
        ..Default::default()
    };
    Ok(OfflineJob {
        schema: "ddi-danoni-offline-job/1".into(),
        roots,
        work: WorkRequest {
            manifest,
            reviews,
            media,
            options,
        },
    })
}

/// Fresh offline generation from explicit accepted codec-stage inputs. Existing
/// exports/library files are never replaced or promoted by this function.
pub fn rebuild_accepted(
    registry: &Path,
    exports: &Path,
    output: &Path,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<serde_json::Value> {
    let raw = bounded_file(registry, JSON_LIMIT)?;
    let document: serde_json::Value = json(&raw)?;
    if document["schema"] != "ddi-danoni-export-registry/1" {
        return Err(Error::new("unsupported-schema", "Unsupported registry"));
    }
    let accepted_index = bounded_file(&exports.join("index.json"), JSON_LIMIT)?;
    let accepted: serde_json::Value = json(&accepted_index)?;
    if accepted["schema"] != "ddi-danoni-export-generation/1"
        || accepted["registrySha256"] != sha256(&raw)
    {
        return Err(Error::new(
            "changed-registry",
            "Registry differs from accepted generation",
        ));
    }
    let rows = document["works"]
        .as_array()
        .ok_or_else(|| Error::new("invalid-document", "Missing registry works"))?;
    let selected = rows
        .iter()
        .filter(|w| w["required"] == true)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(Error::new("empty-generation", "No required sources"));
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(io)?;
    external(&parent)?;
    let output = parent.join(
        output
            .file_name()
            .ok_or_else(|| Error::new("unsafe-output", "Missing generation name"))?,
    );
    fs::create_dir(&output).map_err(io)?;
    let mut guard = Incomplete {
        path: output.clone(),
        armed: true,
    };
    fs::create_dir(output.join("works")).map_err(io)?;
    fs::create_dir(output.join("jobs")).map_err(io)?;
    let mut reports = Vec::new();
    let (mut songs, mut charts) = (0, 0);
    for (i, row) in selected.iter().enumerate() {
        control(Progress {
            stage: Stage::ValidateSource,
            completed: i,
            total: selected.len(),
        })?;
        let id = text(row, "id")?;
        let job = accepted_job(registry, exports, id)?;
        let source = DirectorySource::new(job.roots.clone())?;
        let prepared = prepare_work(&job.work, &source, control)?;
        let result = prepared.manifest["songs"].as_array().unwrap();
        songs += result.len();
        charts += result
            .iter()
            .map(|s| s["charts"].as_array().unwrap().len())
            .sum::<usize>();
        write_prepared(&prepared, &output.join("works").join(id), control)?;
        let body = serde_json::to_vec_pretty(&job)
            .map_err(|e| Error::new("output-json", e.to_string()))?;
        fs::write(output.join("jobs").join(format!("{id}.json")), body).map_err(io)?;
        reports.push(serde_json::json!({"id":id,"collection":row["collection"],"manifest":format!("works/{id}/manifest.json"),"sourceManifestSha256":prepared.manifest["sourceManifestSha256"]}));
    }
    if bounded_file(registry, JSON_LIMIT)? != raw
        || bounded_file(&exports.join("index.json"), JSON_LIMIT)? != accepted_index
    {
        return Err(Error::new(
            "changed-registry",
            "Registry or accepted generation changed during rebuild",
        ));
    }
    let report = serde_json::json!({"schema":"ddi-danoni-rust-offline-generation/1","registrySha256":sha256(&raw),"acceptedGenerationSha256":sha256(&accepted_index),"works":reports,"songs":songs,"charts":charts,"pending":rows.iter().filter(|w|w["required"]!=true).map(|w|serde_json::json!({"id":w["id"],"reason":w["reason"]})).collect::<Vec<_>>(),"mediaPolicy":"supplied accepted media; no decoding/encoding or automatic BPM analysis","networkRequests":0,"workExecutions":0});
    control(Progress {
        stage: Stage::Complete,
        completed: selected.len(),
        total: selected.len(),
    })?;
    fs::write(
        output.join(".manifest.tmp"),
        serde_json::to_vec_pretty(&report).map_err(|e| Error::new("output-json", e.to_string()))?,
    )
    .map_err(io)?;
    fs::rename(output.join(".manifest.tmp"), output.join("manifest.json")).map_err(io)?;
    guard.armed = false;
    Ok(report)
}

/// Native artwork job. Fonts are supplied as hashed local assets, never bundled.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtworkJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub title: Option<crate::artwork::TitleRequest>,
    pub panel: Option<crate::artwork::PanelRequest>,
}
pub fn generate_artwork(job: &Path, output: &Path) -> Result<serde_json::Value> {
    let path = job.canonicalize().map_err(io)?;
    external(&path)?;
    let job: ArtworkJob = json(&bounded_file(&path, JSON_LIMIT)?)?;
    if job.schema != "ddi-danoni-artwork-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let roots = job
        .roots
        .into_iter()
        .map(|(k, p)| {
            (
                k,
                if p.is_absolute() {
                    p
                } else {
                    path.parent().unwrap().join(p)
                },
            )
        })
        .collect();
    let source = DirectorySource::new(roots)?;
    let art = match (job.title, job.panel) {
        (Some(r), None) => crate::artwork::title_cards(&r, &source)?,
        (None, Some(r)) => crate::artwork::title_panel(&r, &source)?,
        _ => {
            return Err(Error::new(
                "invalid-artwork",
                "Select exactly one artwork producer",
            ));
        }
    };
    write_artwork(&art, output)?;
    Ok(art.manifest)
}
pub fn write_artwork(art: &crate::artwork::Artwork, output: &Path) -> Result<()> {
    let parent = output
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(io)?;
    external(&parent)?;
    let name = output
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::new("unsafe-output", "Missing artwork directory"))?;
    relative_path(name)?;
    let output = parent.join(name);
    fs::create_dir(&output).map_err(io)?;
    let mut guard = Incomplete {
        path: output.clone(),
        armed: true,
    };
    let mut seen = std::collections::BTreeSet::new();
    for file in &art.files {
        if ![
            "banner.png",
            "cover.png",
            "pack.png",
            "panel.png",
            "banner.svg",
            "cover.svg",
            "pack.svg",
        ]
        .contains(&file.path.as_str())
            || !seen.insert(&file.path)
        {
            return Err(Error::new("unsafe-output", "Unexpected artwork filename"));
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(&file.path))
            .map_err(io)?;
        f.write_all(&file.bytes).map_err(io)?;
    }
    fs::write(
        output.join(".manifest.tmp"),
        serde_json::to_vec_pretty(&art.manifest)
            .map_err(|e| Error::new("output-json", e.to_string()))?,
    )
    .map_err(io)?;
    fs::rename(output.join(".manifest.tmp"), output.join("manifest.json")).map_err(io)?;
    guard.armed = false;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub request: crate::audio::MediaRequest,
}
pub fn prepare_audio(
    job: &Path,
    output: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<serde_json::Value> {
    let path = job.canonicalize().map_err(io)?;
    external(&path)?;
    let job: AudioJob = json(&bounded_file(&path, JSON_LIMIT)?)?;
    if job.schema != "ddi-danoni-audio-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let roots = job
        .roots
        .into_iter()
        .map(|(k, p)| {
            (
                k,
                if p.is_absolute() {
                    p
                } else {
                    path.parent().unwrap().join(p)
                },
            )
        })
        .collect();
    let source = DirectorySource::new(roots)?;
    let media = crate::audio::prepare_media(&job.request, &source, checkpoint)?;
    write_media(&media, output, checkpoint)?;
    Ok(media.manifest)
}
pub fn write_media(
    media: &crate::audio::PreparedMedia,
    output: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let parent = output
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(io)?;
    external(&parent)?;
    let name = output
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::new("unsafe-output", "Missing media directory"))?;
    relative_path(name)?;
    if media.manifest["schema"] != "ddi-danoni-prepared-media/1" {
        return Err(Error::new("unsupported-schema", "Unexpected media result"));
    }
    let inventory = media.manifest["artifacts"]
        .as_array()
        .ok_or_else(|| Error::new("invalid-inventory", "Missing media inventory"))?;
    if inventory.len() != media.files.len() {
        return Err(Error::new("invalid-inventory", "Wrong media coverage"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for f in &media.files {
        if ![
            "source.bin",
            "source-manifest.json",
            "pcm.wav",
            "music.wav",
            "music.ogg",
        ]
        .contains(&f.path.as_str())
            || !seen.insert(&f.path)
            || !inventory.iter().any(|v| {
                v["path"] == f.path
                    && v["sha256"] == sha256(&f.bytes)
                    && v["bytes"] == f.bytes.len()
            })
        {
            return Err(Error::new("changed-artifact", "Media inventory differs"));
        }
    }
    checkpoint()?;
    let output = parent.join(name);
    fs::create_dir(&output).map_err(io)?;
    let mut guard = Incomplete {
        path: output.clone(),
        armed: true,
    };
    for f in &media.files {
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(&f.path))
            .map_err(io)?;
        for chunk in f.bytes.chunks(1024 * 1024) {
            checkpoint()?;
            target.write_all(chunk).map_err(io)?;
        }
    }
    checkpoint()?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join(".manifest.tmp"))
        .map_err(io)?;
    serde_json::to_writer_pretty(&mut f, &media.manifest)
        .map_err(|e| Error::new("output-json", e.to_string()))?;
    f.sync_all().map_err(io)?;
    drop(f);
    fs::rename(output.join(".manifest.tmp"), output.join("manifest.json")).map_err(io)?;
    guard.armed = false;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TempoJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub request: crate::tempo::TempoRequest,
}
pub fn estimate_tempo(
    job: &Path,
    output: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<serde_json::Value> {
    let path = job.canonicalize().map_err(io)?;
    external(&path)?;
    let job: TempoJob = json(&bounded_file(&path, JSON_LIMIT)?)?;
    if job.schema != "ddi-danoni-tempo-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let roots = job
        .roots
        .into_iter()
        .map(|(k, p)| {
            (
                k,
                if p.is_absolute() {
                    p
                } else {
                    path.parent().unwrap().join(p)
                },
            )
        })
        .collect();
    let source = DirectorySource::new(roots)?;
    let result = crate::tempo::prepare_tempo(&job.request, &source, checkpoint)?;
    let parent = output
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(io)?;
    external(&parent)?;
    let name = output
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::new("unsafe-output", "Missing tempo output name"))?;
    relative_path(name)?;
    checkpoint()?;
    let output = parent.join(name);
    fs::create_dir(&output).map_err(io)?;
    let mut guard = Incomplete {
        path: output.clone(),
        armed: true,
    };
    if !result["review"].is_null() {
        fs::write(
            output.join("grid.json"),
            serde_json::to_vec_pretty(&result["review"])
                .map_err(|e| Error::new("output-json", e.to_string()))?,
        )
        .map_err(io)?;
    }
    checkpoint()?;
    fs::write(
        output.join(".manifest.tmp"),
        serde_json::to_vec_pretty(&result).map_err(|e| Error::new("output-json", e.to_string()))?,
    )
    .map_err(io)?;
    fs::rename(output.join(".manifest.tmp"), output.join("manifest.json")).map_err(io)?;
    guard.armed = false;
    Ok(result)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionJob {
    pub schema: String,
    pub roots: BTreeMap<String, PathBuf>,
    pub request: crate::extraction::ProjectionRequest,
}
/// Passive saved-state job; this command never launches the native worker.
pub fn project_native(
    job: &Path,
    output: &Path,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<serde_json::Value> {
    let path = job.canonicalize().map_err(io)?;
    external(&path)?;
    let job: ProjectionJob = json(&bounded_file(&path, JSON_LIMIT)?)?;
    if job.schema != "ddi-danoni-native-projection-job/1" {
        return Err(Error::new("unsupported-schema", job.schema));
    }
    let roots = job
        .roots
        .into_iter()
        .map(|(k, p)| {
            (
                k,
                if p.is_absolute() {
                    p
                } else {
                    path.parent().unwrap().join(p)
                },
            )
        })
        .collect();
    let source = DirectorySource::new(roots)?;
    let result = crate::extraction::prepare_projection(&job.request, &source, checkpoint)?;
    let parent = output
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(io)?;
    external(&parent)?;
    let name = output
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::new("unsafe-output", "Missing projection output name"))?;
    relative_path(name)?;
    checkpoint()?;
    let output = parent.join(name);
    fs::create_dir(&output).map_err(io)?;
    let mut guard = Incomplete {
        path: output.clone(),
        armed: true,
    };
    fs::write(
        output.join(".manifest.tmp"),
        serde_json::to_vec_pretty(&result).map_err(|e| Error::new("output-json", e.to_string()))?,
    )
    .map_err(io)?;
    checkpoint()?;
    fs::rename(output.join(".manifest.tmp"), output.join("manifest.json")).map_err(io)?;
    guard.armed = false;
    Ok(result)
}
