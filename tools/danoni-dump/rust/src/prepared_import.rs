//! Validated, immutable song handover to a host library. No I/O or installation.
use crate::{
    Error, Result,
    assets::*,
    work::{self, WorkRequest},
};
use ddi_library::{
    manifest::{self, EntryMeta, ManifestEntry},
    pack,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-prepared-import", message)
}
// Sort recursively rather than depending on serde_json's optional preserve_order
// feature, which a desktop host can enable through Cargo feature unification.
fn encoded(value: &impl Serialize) -> Result<Vec<u8>> {
    fn write(value: &Value, out: &mut Vec<u8>) -> Result<()> {
        match value {
            Value::Object(map) => {
                out.push(b'{');
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                for (i, key) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    serde_json::to_writer(&mut *out, key).map_err(|e| invalid(e.to_string()))?;
                    out.push(b':');
                    write(&map[key], out)?;
                }
                out.push(b'}');
            }
            Value::Array(values) => {
                out.push(b'[');
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    write(v, out)?;
                }
                out.push(b']');
            }
            _ => serde_json::to_writer(&mut *out, value).map_err(|e| invalid(e.to_string()))?,
        }
        Ok(())
    }
    let value = serde_json::to_value(value).map_err(|e| invalid(e.to_string()))?;
    let mut out = Vec::new();
    write(&value, &mut out)?;
    Ok(out)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceIdentity {
    pub movie_url: String,
    pub flash_vars: BTreeMap<String, String>,
}
impl SourceIdentity {
    pub fn digest(&self) -> Result<String> {
        let u = url::Url::parse(&self.movie_url).map_err(|e| invalid(e.to_string()))?;
        if self.movie_url.len() > 4096
            || self.movie_url.chars().any(char::is_control)
            || !["http", "https"].contains(&u.scheme())
            || !u.has_host()
            || u.host_str() == Some("web.archive.org")
            || !u.username().is_empty()
            || u.password().is_some()
            || u.fragment().is_some()
            || u.as_str() != self.movie_url
            || self.flash_vars.len() > 128
            || self
                .flash_vars
                .iter()
                .any(|(k, v)| k.len() > 4096 || v.len() > 8192)
        {
            return Err(invalid(
                "Need canonical anonymous logical movie URL and bounded FlashVars",
            ));
        }
        Ok(sha256(&encoded(
            &json!({"schema":"ddi-danoni-source-identity/1","source":self}),
        )?))
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChartValidation {
    pub chart_index: usize,
    pub selector: usize,
    pub endpoints: usize,
    pub maximum_note_error_ms: f64,
    pub native_arrow_heights_per_second: f64,
    pub unrepresented_effects: BTreeMap<String, usize>,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SongSummary {
    pub identity_sha256: String,
    pub source_identity_sha256: String,
    pub encoded_audio_sha256: String,
    pub revision_sha256: String,
    pub selectors: Vec<usize>,
    pub validation: Vec<ChartValidation>,
    pub entry: ManifestEntry,
    pub files: Vec<File>,
}
pub struct PreparedSong {
    summary: SongSummary,
    files: Vec<Artifact>,
}
impl PreparedSong {
    pub fn summary(&self) -> &SongSummary {
        &self.summary
    }
    pub fn files(&self) -> &[Artifact] {
        &self.files
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub schema: &'static str,
    pub digest: String,
    pub source: SourceIdentity,
    pub source_identity_sha256: String,
    pub ready_receipt: Binding,
    pub prepared_receipt: Binding,
    pub musical_quality: &'static str,
    pub tempo_status: String,
    pub songs: Vec<SongSummary>,
}
/// Construct only by validation. Deserializing a summary never creates this proof.
pub struct PreparedImport {
    summary: Summary,
    songs: Vec<PreparedSong>,
    evidence: Vec<Artifact>,
}
impl PreparedImport {
    pub fn summary(&self) -> &Summary {
        &self.summary
    }
    pub fn songs(&self) -> &[PreparedSong] {
        &self.songs
    }
    /// Bound bytes read during admission, relative to the original Ready root.
    /// Full native runtime/acquisition archives remain in the source job directory.
    pub fn evidence(&self) -> &[Artifact] {
        &self.evidence
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Receipts,
    Inventory,
    Replay,
    Library,
    Complete,
}
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub stage: Stage,
    pub completed: usize,
    pub total: usize,
}
fn local(base: &Binding, row: &Value) -> Result<Binding> {
    let b: RelativeBinding =
        serde_json::from_value(row.clone()).map_err(|e| invalid(e.to_string()))?;
    base.sibling(&b)
}
fn normalized_binding(base: &Binding, row: &Value, key: &str) -> Result<Binding> {
    base.sibling(&RelativeBinding {
        path: text(row, key)?.into(),
        sha256: text(row, &format!("{key}Sha256"))?.into(),
    })
}
fn successful(doc: &Value, schema: &str) -> Result<()> {
    if doc["schema"] != schema || doc["ready"] != true || !doc["failure"].is_null() {
        return Err(invalid("Native child is not a successful bound receipt"));
    }
    Ok(())
}
fn checked_doc<S: AssetSource>(inputs: &mut Inputs<'_, S>, b: &Binding) -> Result<Value> {
    json(&inputs.checked(b, JSON_LIMIT)?)
}
struct Archived<'a> {
    files: &'a BTreeMap<String, Arc<Vec<u8>>>,
    manifest: &'a Value,
}
impl AssetSource for Archived<'_> {
    fn read(&self, ns: &str, path: &str, max: usize) -> Result<Vec<u8>> {
        self.read_shared(ns, path, max).map(|b| b.as_ref().clone())
    }
    fn read_shared(&self, ns: &str, path: &str, max: usize) -> Result<Arc<Vec<u8>>> {
        let key = format!("{ns}:{path}");
        let name = self.manifest["inputArtifacts"][&key]
            .as_str()
            .ok_or_else(|| invalid(format!("Unbound replay input: {key}")))?;
        self.files
            .get(name)
            .filter(|b| b.len() <= max)
            .cloned()
            .ok_or_else(|| invalid("Missing replay artifact"))
    }
}
fn component(path: &str) -> Result<()> {
    relative_path(path)?;
    if path.contains('/') || path.len() > 180 || path.starts_with('.') {
        return Err(invalid("Expected portable file component"));
    }
    Ok(())
}
fn selector(id: &str) -> Result<usize> {
    let n = id
        .strip_prefix("chart-")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v < 16)
        .ok_or_else(|| invalid("Unsupported native selector identity"))?;
    if id != format!("chart-{n}") {
        return Err(invalid("Noncanonical selector identity"));
    }
    Ok(n)
}
/// Accepts the current native URL-prepared schema. Receipts bind trusted-producer
/// output bytes; they are not publisher signatures or independent runtime replay.
pub fn validate(
    ready: &Binding,
    source: &impl AssetSource,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<PreparedImport> {
    let mut event = |stage, completed, total| {
        control(Progress {
            stage,
            completed,
            total,
        })
    };
    event(Stage::Receipts, 0, 1)?;
    let mut inputs = Inputs::new(source);
    let outer = checked_doc(&mut inputs, ready)?;
    if outer["schema"] != "ddi-danoni-url-prepared-run/1"
        || outer["ready"] != true
        || !outer["failure"].is_null()
        || outer["completed"].is_null()
    {
        return Err(invalid("Need successful outer URL-prepared Ready receipt"));
    }
    let prepared_binding = local(ready, &outer["completed"]["prepared"])?;
    let prepared = checked_doc(&mut inputs, &prepared_binding)?;
    let source_binding = local(ready, &outer["completed"]["source"])?;
    let native = checked_doc(&mut inputs, &source_binding)?;
    if prepared["schema"] != "ddi-danoni-prepared-work/1"
        || native["schema"] != "ddi-danoni-url-native-run/1"
        || native["ready"] != true
        || !native["failure"].is_null()
    {
        return Err(invalid("Prepared/source child receipt differs"));
    }
    let normalized = &native["completed"]["normalized"];
    let run_binding = local(&source_binding, &native["completed"]["receipt"])?;
    let run = checked_doc(&mut inputs, &run_binding)?;
    successful(&run, "ddi-danoni-native-profile-dependency-run/1")?;
    let profile_binding = local(&run_binding, &run["completed"]["receipt"])?;
    let profile = checked_doc(&mut inputs, &profile_binding)?;
    successful(&profile, "ddi-danoni-native-profile-run/1")?;
    for key in ["manifest", "normalization"] {
        let expected = normalized_binding(&source_binding, normalized, key)?;
        for (base, row) in [
            (&run_binding, &run["completed"]["normalized"]),
            (&profile_binding, &profile["normalized"]),
        ] {
            let actual = normalized_binding(base, row, key)?;
            if expected.namespace != actual.namespace
                || expected.path != actual.path
                || expected.sha256 != actual.sha256
            {
                return Err(invalid(
                    "Source/runtime/profile normalized identities differ",
                ));
            }
        }
    }
    let manifest = source_binding.sibling(&RelativeBinding {
        path: text(normalized, "manifest")?.into(),
        sha256: text(normalized, "manifestSha256")?.into(),
    })?;
    let normalized_doc = checked_doc(&mut inputs, &manifest)?;
    if normalized_doc["schema"] != "ddi-normalized-work/experimental-1"
        || prepared["sourceManifestSha256"] != manifest.sha256
        || prepared["sourceManifest"] != normalized_doc
    {
        return Err(invalid("Prepared songs do not bind normalized source"));
    }
    let origin_binding = source_binding.sibling(&RelativeBinding {
        path: text(normalized, "normalization")?.into(),
        sha256: text(normalized, "normalizationSha256")?.into(),
    })?;
    let origin = checked_doc(&mut inputs, &origin_binding)?;
    if origin["schema"] != "ddi-danoni-native-normalization/1"
        || origin["normalizedManifestSha256"] != manifest.sha256
    {
        return Err(invalid("Native origin receipt differs"));
    }
    let context_rows = native["inputs"]
        .as_array()
        .filter(|a| a.len() <= 4096)
        .ok_or_else(|| invalid("Missing bounded source inventory"))?;
    let context_rows = context_rows
        .iter()
        .filter(|b| b["namespace"] == "url" && b["path"] == "context.json")
        .collect::<Vec<_>>();
    if context_rows.len() != 1 {
        return Err(invalid("Need unique source-bound runtime context"));
    }
    let context_binding = source_binding.sibling(&RelativeBinding {
        path: "acquisition/context.json".into(),
        sha256: text(context_rows[0], "sha256")?.into(),
    })?;
    let context = checked_doc(&mut inputs, &context_binding)?;
    let identity = SourceIdentity {
        movie_url: text(&context, "sourceUrl")?.into(),
        flash_vars: serde_json::from_value(context["flashVars"].clone())
            .map_err(|e| invalid(e.to_string()))?,
    };
    let identity_hash = identity.digest()?;
    let inventory = prepared["artifacts"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 16384)
        .ok_or_else(|| invalid("Need bounded artifact inventory"))?;
    let mut files = BTreeMap::new();
    let mut aliases = BTreeSet::new();
    for (i, row) in inventory.iter().enumerate() {
        event(Stage::Inventory, i, inventory.len())?;
        let path = text(row, "path")?;
        relative_path(path)?;
        if !["songs/", "references/", "provenance/"]
            .iter()
            .any(|p| path.starts_with(p))
            || !aliases.insert(path.to_lowercase())
        {
            return Err(invalid("Unsafe or case-colliding artifact inventory"));
        }
        let b = prepared_binding.sibling(&RelativeBinding {
            path: path.into(),
            sha256: text(row, "sha256")?.into(),
        })?;
        let body = inputs.checked(&b, MEDIA_LIMIT)?;
        if row["bytes"] != body.len() {
            return Err(invalid("Artifact byte count changed"));
        }
        files.insert(path.to_owned(), body);
    }
    event(Stage::Replay, 0, 1)?;
    let work_request: WorkRequest =
        serde_json::from_value(prepared["request"].clone()).map_err(|e| invalid(e.to_string()))?;
    if work_request.manifest.sha256 != manifest.sha256 {
        return Err(invalid("Work request belongs to another source"));
    }
    let replay = work::prepare_work(
        &work_request,
        &Archived {
            files: &files,
            manifest: &prepared,
        },
        &mut |p| event(Stage::Replay, p.completed, p.total),
    )?;
    for file in &replay.files {
        let archived = files
            .get(&file.path)
            .ok_or_else(|| invalid("Missing rebuilt core artifact"))?;
        if archived != &file.bytes {
            // Source/playback bytes remain exact. These generated reference
            // wrappers can differ only in JSON object ordering chosen by a host.
            if !file.path.starts_with("references/")
                || !file.path.ends_with(".json")
                || json::<Value>(archived)? != json::<Value>(&file.bytes)?
            {
                return Err(invalid("Rebuilt core song artifact differs"));
            }
        }
    }
    let mut core = prepared.clone();
    core.as_object_mut().unwrap().remove("preparation");
    core["artifacts"] = replay.manifest["artifacts"].clone();
    if core != replay.manifest {
        return Err(invalid("Rebuilt work metadata differs"));
    }
    let proof_binding = local(
        &prepared_binding,
        &json!({"path":prepared["preparation"]["path"],"sha256":prepared["preparation"]["sha256"]}),
    )?;
    let proof = checked_doc(&mut inputs, &proof_binding)?;
    let tempo = text(&proof["tempo"], "status")?;
    if proof["schema"] != "ddi-danoni-fresh-preparation/1"
        || proof["sourceManifestSha256"] != manifest.sha256
        || !["estimated", "exact-source-fallback"].contains(&tempo)
        || prepared["preparation"]["tempoStatus"] != tempo
        || prepared["preparation"]["musicalQuality"] != "provisional"
    {
        return Err(invalid("Fresh preparation quality/source differs"));
    }
    if files.len() != replay.files.len() + 1
        || !files.contains_key(text(&prepared["preparation"], "path")?)
    {
        return Err(invalid("Unexpected supplemental artifacts"));
    }
    let rows = origin["charts"]
        .as_array()
        .filter(|a| a.len() == normalized_doc["charts"].as_array().map_or(0, Vec::len))
        .ok_or_else(|| invalid("Origin chart coverage differs"))?;
    let mut origins = BTreeMap::new();
    for row in rows {
        let id = text(row, "id")?;
        let _ = selector(id)?;
        let hash = text(row, "sourceMp3Sha256")?;
        if !valid_hash(hash)
            || row["logicalClockAgreement"] != true
            || origins.insert(id.to_owned(), row).is_some()
        {
            return Err(invalid("Ambiguous source encoded-audio identity"));
        }
    }
    let paths = files.keys().cloned().collect::<Vec<_>>();
    let candidates = pack::find_songs(&paths, "DanOni");
    let songs = prepared["songs"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 128)
        .ok_or_else(|| invalid("Need bounded song list"))?;
    if candidates.len() != songs.len() || outer["completed"]["songs"] != songs.len() {
        return Err(invalid("Library scanner song coverage differs"));
    }
    let mut result = Vec::new();
    let mut ids = BTreeSet::new();
    let mut seen_charts = BTreeSet::new();
    for (i, song) in songs.iter().enumerate() {
        event(Stage::Library, i, songs.len())?;
        let candidate = candidates
            .iter()
            .find(|c| c.chart == song["simfile"])
            .ok_or_else(|| invalid("Song SSC is not the library scanner choice"))?;
        let parsed = ddi_chart::formats::sm::parse_ssc(
            std::str::from_utf8(&files[&candidate.chart]).map_err(|e| invalid(e.to_string()))?,
        )
        .map_err(|e| invalid(e.to_string()))?;
        let assets = pack::resolve_assets(&parsed, &candidate.dir, &paths);
        if assets.music.as_deref() != song["music"].as_str()
            || assets.banner.as_deref() != song["banner"].as_str()
            || assets.background.is_some()
            || !parsed.effects.is_empty()
        {
            return Err(invalid(
                "Library resolves unexpected or unsupported playback assets",
            ));
        }
        let reports = song["charts"]
            .as_array()
            .filter(|a| a.len() == parsed.charts.len())
            .ok_or_else(|| invalid("Chart coverage differs"))?;
        let mut selectors = Vec::new();
        let mut validation = Vec::new();
        let mut encoded_hash = None;
        for (chart_index, (chart, report)) in parsed.charts.iter().zip(reports).enumerate() {
            let id = text(report, "id")?;
            let n = selector(id)?;
            if !crate::model::Mode::is_playable_label(text(report, "mode")?)
                || !seen_charts.insert(id.to_owned())
                || chart.layout != format!("danoni-{}", text(report, "mode")?)
                || parsed.layout_of(chart).is_none()
            {
                return Err(invalid("Unsupported or repeated chart selector/layout"));
            }
            let origin = origins
                .get(id)
                .ok_or_else(|| invalid("Missing chart audio origin"))?;
            if origin["canonicalSha256"] != song["sourceAudioSha256"] {
                return Err(invalid("Song audio origin differs"));
            }
            let h = text(origin, "sourceMp3Sha256")?;
            if encoded_hash.is_some_and(|e| e != h) {
                return Err(invalid(
                    "Shared playback has different encoded source identities",
                ));
            }
            encoded_hash = Some(h);
            let maximum = report["maximumNoteErrorMs"]
                .as_f64()
                .filter(|n| n.is_finite() && (0.0..=9.0000001).contains(n))
                .ok_or_else(|| invalid("Invalid endpoint error report"))?;
            let rate = report["nativeMotion"]["baseArrowHeightsPerSecond"]
                .as_f64()
                .filter(|n| n.is_finite() && *n > 0.0)
                .ok_or_else(|| invalid("Missing observed native movement"))?;
            validation.push(ChartValidation {
                chart_index,
                selector: n,
                endpoints: report["endpoints"]
                    .as_array()
                    .ok_or_else(|| invalid("Missing endpoints"))?
                    .len(),
                maximum_note_error_ms: maximum,
                native_arrow_heights_per_second: rate,
                unrepresented_effects: serde_json::from_value(
                    report["unrepresentedEffects"].clone(),
                )
                .map_err(|e| invalid(e.to_string()))?,
            });
            selectors.push(n);
        }
        selectors.sort_unstable();
        let audio_hash = encoded_hash.ok_or_else(|| invalid("Song has no supported charts"))?;
        let song_identity = sha256(&encoded(
            &json!({"schema":"ddi-danoni-song-identity/1","sourceIdentitySha256":identity_hash,"encodedAudioSha256":audio_hash}),
        )?);
        let id = format!("u-{song_identity}");
        if !ids.insert(id.clone()) {
            return Err(invalid(
                "Ambiguous song identities; explicit stream discrimination required",
            ));
        }
        let prefix = format!("{}/", candidate.dir);
        let mut playback = Vec::new();
        for path in std::iter::once(&candidate.chart)
            .chain(assets.music.iter())
            .chain(assets.banner.iter())
        {
            let name = path
                .strip_prefix(&prefix)
                .ok_or_else(|| invalid("Playback asset escapes song folder"))?;
            component(name)?;
            playback.push(Artifact {
                path: name.into(),
                bytes: files[path].clone(),
            });
        }
        playback.sort_by(|a, b| a.path.cmp(&b.path));
        let chart_name = candidate.chart.strip_prefix(&prefix).unwrap().to_owned();
        let music = assets
            .music
            .unwrap()
            .strip_prefix(&prefix)
            .unwrap()
            .to_owned();
        let banner = assets
            .banner
            .map(|p| p.strip_prefix(&prefix).unwrap().to_owned());
        let (title, artist) = manifest::simfile_names(&parsed, candidate.dir_name());
        let mut entry = manifest::summarize(
            EntryMeta {
                id,
                title,
                artist,
                chart: chart_name,
                music,
                banner,
                credit: parsed.credit.trim().into(),
                ..Default::default()
            },
            &parsed,
        );
        entry.bpm = match parsed.display_bpm {
            ddi_chart::DisplayBpm::Random => String::new(),
            ddi_chart::DisplayBpm::Single(b) => format!("{}", b.round() as i64),
            ddi_chart::DisplayBpm::Range(a, b) => {
                format!("{}-{}", a.round() as i64, b.round() as i64)
            }
            ddi_chart::DisplayBpm::Actual => entry.bpm,
        };
        if entry.charts.len() != reports.len()
            || !entry.preview_start.is_finite()
            || !entry.preview_length.is_finite()
        {
            return Err(invalid("Library summary is incomplete/nonfinite"));
        }
        let inventory = playback
            .iter()
            .map(|f| File {
                path: f.path.clone(),
                sha256: sha256(&f.bytes),
                bytes: f.bytes.len(),
            })
            .collect::<Vec<_>>();
        let revision = sha256(&encoded(
            &json!({"schema":"ddi-danoni-song-revision/1","identity":song_identity,"entry":entry,"selectors":selectors,"validation":validation,"files":inventory,"musicalQuality":"provisional","tempoStatus":tempo}),
        )?);
        result.push(PreparedSong {
            summary: SongSummary {
                identity_sha256: song_identity,
                source_identity_sha256: identity_hash.clone(),
                encoded_audio_sha256: audio_hash.into(),
                revision_sha256: revision,
                selectors,
                validation,
                entry,
                files: inventory,
            },
            files: playback,
        });
    }
    if seen_charts.len() != origins.len() || outer["completed"]["charts"] != seen_charts.len() {
        return Err(invalid("Native/prepared chart coverage differs"));
    }
    result.sort_by(|a, b| a.summary.entry.id.cmp(&b.summary.entry.id));
    let summaries = result.iter().map(|s| s.summary.clone()).collect::<Vec<_>>();
    let digest = sha256(&encoded(
        &json!({"schema":"ddi-danoni-import-content/1","sourceIdentitySha256":identity_hash,"songs":summaries,"musicalQuality":"provisional"}),
    )?);
    let prefix = ready.path.rsplit_once('/').map_or("", |(p, _)| p);
    let mut evidence = Vec::new();
    for b in inputs.used.clone().values() {
        let body = inputs.checked(b, MEDIA_LIMIT)?;
        let p = if prefix.is_empty() {
            b.path.as_str()
        } else {
            b.path
                .strip_prefix(&format!("{prefix}/"))
                .ok_or_else(|| invalid("Evidence escapes Ready root"))?
        };
        evidence.push(Artifact {
            path: p.into(),
            bytes: body,
        });
    }
    evidence.sort_by(|a, b| a.path.cmp(&b.path));
    let summary = Summary {
        schema: "ddi-danoni-prepared-import/1",
        digest,
        source: identity,
        source_identity_sha256: identity_hash,
        ready_receipt: ready.clone(),
        prepared_receipt: prepared_binding,
        musical_quality: "provisional",
        tempo_status: tempo.into(),
        songs: summaries,
    };
    event(Stage::Complete, 1, 1)?;
    Ok(PreparedImport {
        summary,
        songs: result,
        evidence,
    })
}
#[cfg(feature = "native")]
pub fn load_ready(
    root: &std::path::Path,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<PreparedImport> {
    let source = crate::native::DirectorySource::new(BTreeMap::from([(
        "candidate".into(),
        root.to_owned(),
    )]))?;
    let body = source.read("candidate", "manifest.json", JSON_LIMIT)?;
    validate(
        &Binding {
            namespace: "candidate".into(),
            path: "manifest.json".into(),
            sha256: sha256(&body),
        },
        &source,
        control,
    )
}
