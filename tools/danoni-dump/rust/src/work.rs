//! Complete offline song assembly from source-bound reviews and supplied media.
use crate::{
    Error, Result,
    assets::*,
    model::*,
    preview::{self, PreviewSettings},
    reviews::{self, Entry},
    ssc,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReviewBindings {
    pub grid: Option<Binding>,
    pub motion: Option<Binding>,
    pub ratings: Option<Binding>,
    pub artwork: Option<Binding>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Wav,
    /// Ogg Vorbis, retained for other-player compatibility.
    Ogg,
    /// Ogg Opus (48 kHz playback, source PCM retained for analysis).
    Opus,
}
impl Codec {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Ogg => "ogg",
            Self::Opus => "opus",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SuppliedMedia {
    pub source_audio_sha256: String,
    pub audio: Binding,
    pub pcm: Binding,
    pub codec: Codec,
    /// Exact prior preparation receipt binding original input and supplied media.
    pub provenance: Binding,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct WorkOptions {
    pub tier_policy: TierPolicy,
    pub tier_bounds: [u16; 4],
    pub scroll_factor: f64,
    pub preview: PreviewSettings,
    pub display_bpms: BTreeMap<String, String>,
}
impl Default for WorkOptions {
    fn default() -> Self {
        Self {
            tier_policy: TierPolicy::Stars,
            tier_bounds: [1, 3, 5, 7],
            scroll_factor: 1.0,
            preview: PreviewSettings::default(),
            display_bpms: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkRequest {
    pub manifest: Binding,
    #[serde(default)]
    pub reviews: ReviewBindings,
    pub media: Vec<SuppliedMedia>,
    #[serde(default)]
    pub options: WorkOptions,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    ValidateSource,
    ValidateReviews,
    PrepareAudio,
    PrepareCharts,
    Assemble,
    Complete,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub completed: usize,
    pub total: usize,
}
/// An immutable staged result. The host chooses when/how to update its library.
pub struct PreparedWork {
    pub manifest: Value,
    pub files: Vec<Artifact>,
}
fn invalid(message: &str) -> Error {
    Error::new("invalid-work", message)
}
fn event(
    control: &mut impl FnMut(Progress) -> Result<()>,
    stage: Stage,
    completed: usize,
    total: usize,
) -> Result<()> {
    control(Progress {
        stage,
        completed,
        total,
    })
}
fn artifact(
    files: &mut BTreeMap<String, Artifact>,
    path: String,
    bytes: Arc<Vec<u8>>,
) -> Result<()> {
    relative_path(&path)?;
    if let Some(old) = files.get(&path) {
        if old.bytes != bytes {
            return Err(invalid("Conflicting output artifacts"));
        }
    } else {
        files.insert(path.clone(), Artifact { path, bytes });
    }
    Ok(())
}
fn json_bytes(value: &Value) -> Result<Arc<Vec<u8>>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| invalid(&e.to_string()))?;
    bytes.push(b'\n');
    Ok(Arc::new(bytes))
}
fn metadata_string<'a>(m: &'a Metadata, key: &str, default: &'a str) -> Result<&'a str> {
    match m.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_str()
            .ok_or_else(|| invalid("Song metadata must be text")),
    }
}

pub fn display_bpm(raw: &str, source: &str) -> Result<Value> {
    if raw == "*" {
        return Ok(json!({"kind":"unknown","sscValue":"*","source":source}));
    }
    let parts = raw
        .split(':')
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| invalid("Invalid display BPM"))?;
    if !(1..=2).contains(&parts.len())
        || parts
            .iter()
            .any(|v| !v.is_finite() || !(0.0 < *v && *v <= 1e6))
        || parts[0] > *parts.last().unwrap()
    {
        return Err(invalid(
            "Display BPM requires finite positive increasing values",
        ));
    }
    Ok(
        json!({"kind":if parts.len()==1{"single"}else{"range"},"minimumBpm":parts[0],"maximumBpm":parts.last(),"sscValue":parts.iter().map(ToString::to_string).collect::<Vec<_>>().join(":"),"source":source}),
    )
}
fn authored_bpm(raw: Option<&Value>) -> Option<Value> {
    let raw = raw?.as_str()?;
    let parts = raw.split('-').collect::<Vec<_>>();
    if !(1..=2).contains(&parts.len())
        || parts.iter().any(|p| {
            let split = p.split('.').collect::<Vec<_>>();
            !(1..=2).contains(&split.len())
                || split
                    .iter()
                    .any(|v| v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()))
        })
    {
        return None;
    }
    display_bpm(&raw.replace('-', ":"), "authored CW header").ok()
}

pub(crate) fn entries<S: AssetSource>(
    raw: &Value,
    binding: &Binding,
    inputs: &mut Inputs<'_, S>,
) -> Result<Vec<Entry>> {
    if ![
        "ddi-normalized-work/experimental-1",
        "ddi-legacy-export/experimental-1",
        "ddi-synthetic-export/experimental-1",
    ]
    .contains(&text(raw, "schema")?)
    {
        return Err(invalid("Unsupported normalized source schema"));
    }
    let charts = raw["charts"]
        .as_array()
        .ok_or_else(|| invalid("Missing normalized charts"))?;
    if charts.is_empty() || charts.len() > 2048 {
        return Err(invalid("Need 1..=2048 source charts"));
    }
    let defaults = raw
        .get("metadata")
        .map(|m| {
            m.as_object()
                .ok_or_else(|| invalid("Invalid default metadata"))
        })
        .transpose()?
        .cloned()
        .unwrap_or_default();
    let mut result = Vec::new();
    let mut ids = BTreeSet::new();
    for value in charts {
        let chart: Chart =
            serde_json::from_value(value.clone()).map_err(|e| invalid(&e.to_string()))?;
        if chart.id.is_empty() || !ids.insert(chart.id.clone()) {
            return Err(invalid("Chart ids must be unique and nonempty"));
        }
        let music = value
            .get("music")
            .or_else(|| raw.get("music"))
            .ok_or_else(|| invalid("Missing chart music"))?;
        let music = RelativeBinding {
            path: text(music, "path")?.into(),
            sha256: text(music, "sha256")?.into(),
        };
        inputs.local(binding, &music.path, &music.sha256, MEDIA_LIMIT)?;
        let reference_path = value
            .get("nativeReference")
            .or_else(|| raw.get("evidence").and_then(|v| v.get("reference")));
        let reference: Value = if let Some(path) = reference_path {
            let body = inputs.reference(
                binding,
                path.as_str()
                    .ok_or_else(|| invalid("Invalid reference path"))?,
            )?;
            json(&body)?
        } else {
            json!({})
        };
        let mut metadata = defaults.clone();
        if let Some(m) = reference.get("metadata") {
            metadata.extend(
                m.as_object()
                    .ok_or_else(|| invalid("Invalid reference metadata"))?
                    .clone(),
            );
        }
        metadata.extend(chart.metadata.clone());
        let stars = crate::policy::source_stars(&chart, &metadata)?;
        result.push(Entry {
            chart,
            raw_chart: value.clone(),
            metadata,
            reference,
            music,
            source_stars: stars,
        });
    }
    Ok(result)
}

/// No decoding/fitting/fetching: supplied media is reused and its provenance
/// checked. The control callback may cancel before/between preparation steps.
pub fn prepare_work<S: AssetSource>(
    request: &WorkRequest,
    source: &S,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<PreparedWork> {
    event(control, Stage::ValidateSource, 0, 1)?;
    let mut inputs = Inputs::new(source);
    let manifest_bytes = inputs.checked(&request.manifest, JSON_LIMIT)?;
    let manifest: Value = json(&manifest_bytes)?;
    let hash = sha256(&manifest_bytes);
    let entries = entries(&manifest, &request.manifest, &mut inputs)?;
    crate::policy::tier_for_stars(None, request.options.tier_bounds)?;
    event(control, Stage::ValidateReviews, 0, entries.len())?;
    let reviews = reviews::load(
        &mut inputs,
        &hash,
        &entries,
        manifest["sourceSwfSha256"].as_str(),
        &request.reviews,
    )?;
    if reviews.motions.is_empty() {
        crate::policy::base_ratio(None, 75.0, request.options.scroll_factor)?;
    }
    let mut display = BTreeMap::new();
    for (id, raw) in &request.options.display_bpms {
        if !entries.iter().any(|e| &e.chart.id == id) {
            return Err(invalid("Display BPM selects an unknown chart"));
        }
        display.insert(id.clone(), display_bpm(raw, "explicit review input")?);
    }
    for entry in &entries {
        if !display.contains_key(&entry.chart.id)
            && let Some(bpm) = authored_bpm(entry.metadata.get("declaredDisplayBpm"))
        {
            display.insert(entry.chart.id.clone(), bpm);
        }
    }
    let mut groups: Vec<Vec<&Entry>> = Vec::new();
    for entry in &entries {
        if let Some(group) = groups
            .iter_mut()
            .find(|g| g[0].music.sha256 == entry.music.sha256)
        {
            group.push(entry);
        } else {
            groups.push(vec![entry]);
        }
    }
    let expected = groups
        .iter()
        .map(|g| g[0].music.sha256.as_str())
        .collect::<BTreeSet<_>>();
    let provided = request
        .media
        .iter()
        .map(|m| m.source_audio_sha256.as_str())
        .collect::<BTreeSet<_>>();
    if expected != provided || request.media.len() != provided.len() {
        return Err(invalid(
            "Supplied media must cover every distinct source audio exactly once",
        ));
    }
    let mut files = BTreeMap::new();
    let mut songs = Vec::new();
    let mut used = BTreeSet::new();
    artifact(
        &mut files,
        "provenance/source-manifest.json".into(),
        manifest_bytes,
    )?;
    for (number, group) in groups.iter().enumerate() {
        event(control, Stage::PrepareAudio, number, groups.len())?;
        let first = group[0];
        let media = request
            .media
            .iter()
            .find(|m| m.source_audio_sha256 == first.music.sha256)
            .unwrap();
        let audio = inputs.checked(&media.audio, MEDIA_LIMIT)?;
        let pcm = inputs.checked(&media.pcm, MEDIA_LIMIT)?;
        let provenance = inputs.checked(&media.provenance, JSON_LIMIT)?;
        let previous: Value = json(&provenance)?;
        if previous["sourceManifestSha256"] != hash {
            return Err(invalid(
                "Supplied media provenance belongs to another normalized source",
            ));
        }
        let envelope = preview::pcm_envelope(&pcm, &mut || {
            event(control, Stage::PrepareAudio, number, groups.len())
        })?;
        match previous["schema"].as_str() {
            Some("ddi-danoni-prepared-media/1") => {
                if previous["sourceAudioSha256"] != media.source_audio_sha256
                    || previous["playbackSha256"] != media.audio.sha256
                    || previous["pcmSha256"] != media.pcm.sha256
                    || previous["sampleRate"] != envelope.sample_rate
                    || previous["channels"] != envelope.channels
                    || previous["sampleFrames"] != envelope.frames
                    || previous["codec"] != media.codec.extension()
                {
                    return Err(invalid(
                        "Supplied media differs from its neutral preparation receipt",
                    ));
                }
            }
            Some("ddi-playable-pack/experimental-1") => {
                let receipt = previous["songs"]
                    .as_array()
                    .and_then(|rs| {
                        rs.iter().find(|r| {
                            r["sourceAudioSha256"] == media.source_audio_sha256
                                && r["musicSha256"] == media.audio.sha256
                        })
                    })
                    .ok_or_else(|| {
                        invalid("Supplied media does not match its legacy preparation receipt")
                    })?;
                if receipt["pcmSampleRate"] != envelope.sample_rate
                    || receipt["pcmChannels"] != envelope.channels
                    || receipt["pcmFrames"] != envelope.frames
                    || previous["codec"] != media.codec.extension()
                {
                    return Err(invalid(
                        "Supplied PCM units or codec differ from legacy media receipt",
                    ));
                }
            }
            _ => return Err(invalid("Unsupported supplied media receipt")),
        }
        match media.codec {
            Codec::Wav => {
                if audio != pcm {
                    return Err(invalid("WAV playback must equal supplied PCM"));
                }
            }
            Codec::Ogg | Codec::Opus => {
                if !audio.starts_with(b"OggS") || previous["codec"] != media.codec.extension() {
                    return Err(invalid("Ogg media has the wrong container or receipt"));
                }
            }
        }
        let charts = group.iter().map(|e| &e.chart).collect::<Vec<_>>();
        let preview =
            preview::choose_preview(&envelope, &charts, &request.options.preview, &mut || {
                event(control, Stage::PrepareAudio, number, groups.len())
            })?;
        let mut id = if first.chart.id.len() <= 128
            && first
                .chart
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            first.chart.id.clone()
        } else {
            format!("song-{}", number + 1)
        };
        let base_id = id.clone();
        let mut suffix = number + 1;
        while used.contains(&id) {
            id = format!("{base_id}-{suffix}");
            suffix += 1;
        }
        used.insert(id.clone());
        let title = metadata_string(
            &first.metadata,
            "title",
            metadata_string(&first.metadata, "musicTitle", &first.chart.id)?,
        )?;
        let artist = metadata_string(&first.metadata, "artist", "")?;
        let grid = reviews.grids.get(&first.chart.id);
        let phase = grid.map_or(0.0, |g| g.beat_zero_seconds);
        let timing_bpm = grid.map_or(75.0, |g| {
            g.evidence
                .get("sharedTempo")
                .and_then(|v| v["consensusBpm"].as_f64())
                .unwrap_or(g.bpm)
        });
        if !timing_bpm.is_finite() || !(60.0..=360.0).contains(&timing_bpm) {
            return Err(invalid("Invalid shared song tempo"));
        }
        let selected = group
            .iter()
            .filter_map(|e| display.get(&e.chart.id))
            .collect::<Vec<_>>();
        if selected.iter().any(|v| *v != selected[0]) {
            return Err(invalid("Conflicting display BPM metadata for shared music"));
        }
        let display = selected.first().copied();
        let selected_art = group
            .iter()
            .map(|e| reviews.artwork.get(&e.chart.id))
            .collect::<Vec<_>>();
        let banner = selected_art.iter().find_map(|v| *v);
        if let Some(banner) = banner
            && selected_art
                .iter()
                .any(|v| v.is_none_or(|v| v.summary != banner.summary || v.bytes != banner.bytes))
        {
            return Err(invalid(
                "Shared-music charts need the same reviewed artwork",
            ));
        }
        let extension = media.codec.extension();
        let music_path = format!("songs/{id}/music.{extension}");
        let mut ssc = format!(
            "#VERSION:0.83;\n#TITLE:{};\n#ARTIST:{};\n#MUSIC:music.{extension};\n#OFFSET:{};\n#BPMS:0={};\n#SAMPLESTART:{};\n#SAMPLELENGTH:{};\n",
            ssc::header_value(title)?,
            ssc::header_value(artist)?,
            -phase,
            timing_bpm,
            preview["startSeconds"].as_f64().unwrap(),
            preview["lengthSeconds"].as_f64().unwrap()
        );
        if banner.is_some() {
            ssc.push_str("#BANNER:banner.png;\n");
        }
        if let Some(display) = display {
            ssc.push_str(&format!("#DISPLAYBPM:{};\n", text(display, "sscValue")?));
        }
        let mut prepared_charts = Vec::new();
        let stars = group
            .iter()
            .map(|e| {
                reviews
                    .ratings
                    .get(&e.chart.id)
                    .and_then(|v| v["meter"].as_u64())
                    .map(|n| n as u16)
                    .or(e.source_stars)
            })
            .collect::<Vec<_>>();
        for (i, entry) in group.iter().enumerate() {
            event(control, Stage::PrepareCharts, i, group.len())?;
            let preserve_label = stars[i].is_some()
                && group
                    .iter()
                    .zip(&stars)
                    .filter(|(e, s)| e.chart.mode == entry.chart.mode && **s == stars[i])
                    .count()
                    > 1;
            let options = PreparationOptions {
                grid: reviews.grids.get(&entry.chart.id).cloned(),
                native_motion: reviews.motions.get(&entry.chart.id).cloned(),
                scroll_factor: request.options.scroll_factor,
                tier_bounds: request.options.tier_bounds,
                tier_policy: request.options.tier_policy,
                rating: RatingPolicy::Reviewed { stars: stars[i] },
                preserve_label,
            };
            let compiled = crate::compile_chart(&entry.chart, &entry.metadata, &options)?;
            ssc.push_str(&compiled.ssc);
            let mut report =
                serde_json::to_value(&compiled).map_err(|e| invalid(&e.to_string()))?;
            report.as_object_mut().unwrap().remove("ssc");
            report["sourceRating"] = reviews
                .ratings
                .get(&entry.chart.id)
                .cloned()
                .unwrap_or(Value::Null);
            report["sourceDifficultyLabel"] = entry
                .metadata
                .get("difficultyLabel")
                .or_else(|| entry.metadata.get("label"))
                .cloned()
                .unwrap_or(Value::Null);
            report["sourceDifficultyClassification"] = entry
                .metadata
                .get("difficultyClassification")
                .cloned()
                .unwrap_or(Value::Null);
            report["preservedDuplicateLabel"] = json!(preserve_label);
            report["musicalGrid"] =
                serde_json::to_value(&options.grid).map_err(|e| invalid(&e.to_string()))?;
            report["nativeMotion"] = serde_json::to_value(&options.native_motion)
                .map_err(|e| invalid(&e.to_string()))?;
            artifact(
                &mut files,
                format!("references/{id}-{i}.json"),
                json_bytes(
                    &json!({"sourceChart":entry.raw_chart,"sourceMetadata":entry.metadata,"sourceReference":entry.reference,"sourceRating":report["sourceRating"]}),
                )?,
            )?;
            prepared_charts.push(report);
        }
        artifact(&mut files, music_path.clone(), audio)?;
        artifact(&mut files, format!("references/{id}.wav"), pcm)?;
        artifact(
            &mut files,
            format!("provenance/media-{id}.json"),
            provenance,
        )?;
        let banner_path = banner.map(|_| format!("songs/{id}/banner.png"));
        if let Some(banner) = banner {
            artifact(
                &mut files,
                banner_path.clone().unwrap(),
                banner.bytes.clone(),
            )?;
        }
        let simfile = format!("songs/{id}/song.ssc");
        artifact(&mut files, simfile.clone(), Arc::new(ssc.into_bytes()))?;
        songs.push(json!({"id":id,"title":title,"artist":artist,"music":music_path,"musicSha256":media.audio.sha256,"sourceAudioSha256":media.source_audio_sha256,"pcmSha256":media.pcm.sha256,"pcmSampleRate":envelope.sample_rate,"pcmChannels":envelope.channels,"pcmFrames":envelope.frames,"banner":banner_path,"artwork":banner.map(|a|&a.summary),"preview":preview,"displayBpm":display,"timingBpm":timing_bpm,"timingOffsetSeconds":-phase,"simfile":simfile,"charts":prepared_charts,"mediaPolicy":"supplied byte-identical prepared media; no new decoding, encoding or origin proof"}));
    }
    event(control, Stage::Assemble, 0, 1)?;
    let mut input_artifacts = BTreeMap::new();
    let mut content_paths = files
        .values()
        .map(|a| (sha256(&a.bytes), a.path.clone()))
        .collect::<BTreeMap<_, _>>();
    for (key, binding) in inputs.used.clone() {
        let path = if let Some(path) = content_paths.get(&binding.sha256) {
            path.clone()
        } else {
            let path = format!("provenance/assets/{}", binding.sha256);
            let body = inputs.checked(&binding, MEDIA_LIMIT)?;
            artifact(&mut files, path.clone(), body)?;
            content_paths.insert(binding.sha256.clone(), path.clone());
            path
        };
        input_artifacts.insert(key, path);
    }
    let inventory = files
        .values()
        .map(|a| json!({"path":a.path,"sha256":sha256(&a.bytes),"bytes":a.bytes.len()}))
        .collect::<Vec<_>>();
    let result = json!({"schema":"ddi-danoni-prepared-work/1","implementation":{"package":"ddi-danoni-export","version":env!("CARGO_PKG_VERSION"),"policy":"offline-song-preparation/1"},"sourceSchema":manifest["schema"],"sourceManifestSha256":hash,"sourceManifest":manifest,"request":request,"inputs":inputs.used,"inputArtifacts":input_artifacts,"songs":songs,"artifacts":inventory,"networkRequests":0,"workExecutions":0});
    event(control, Stage::Complete, 1, 1)?;
    Ok(PreparedWork {
        manifest: result,
        files: files.into_values().collect(),
    })
}
