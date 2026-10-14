//! Fresh source-bound audio, tempo and artwork preparation, without host I/O.
use crate::{
    Error, Result, artwork,
    assets::*,
    audio, tempo,
    work::{self, Codec, PreparedWork, ReviewBindings, SuppliedMedia, WorkOptions, WorkRequest},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
const GENERATED: &str = "ddi-preparation";
type SharedInputs = BTreeMap<(String, String), Arc<Vec<u8>>>;
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-preparation", message)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Options {
    pub codec: Codec,
    pub quality: f32,
    pub work: WorkOptions,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            codec: Codec::Ogg,
            quality: 0.5,
            work: WorkOptions::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub manifest: Binding,
    /// Full source-bound, measured default motion coverage is mandatory.
    pub motion: Binding,
    /// Explicit host font. None leaves artwork absent, with a diagnostic.
    pub font: Option<Binding>,
    #[serde(default)]
    pub options: Options,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Validate,
    Audio,
    Tempo,
    Artwork,
    Songs,
    Complete,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub completed: usize,
    pub total: usize,
}
struct Session<'a, S: AssetSource> {
    source: &'a S,
    cache: RefCell<SharedInputs>,
    generated: BTreeMap<String, Arc<Vec<u8>>>,
    bytes: usize,
}
impl<S: AssetSource> Session<'_, S> {
    fn add(&mut self, path: String, bytes: Arc<Vec<u8>>) -> Result<Binding> {
        relative_path(&path)?;
        if self.generated.contains_key(&path)
            || self.generated.len() >= 16384
            || self
                .bytes
                .checked_add(bytes.len())
                .is_none_or(|n| n > 512 * 1024 * 1024)
        {
            return Err(invalid(
                "Generated stage inputs exceed coverage or 512 MiB limit",
            ));
        }
        self.bytes += bytes.len();
        let binding = Binding {
            namespace: GENERATED.into(),
            path: path.clone(),
            sha256: sha256(&bytes),
        };
        self.generated.insert(path, bytes);
        Ok(binding)
    }
    fn json(&mut self, path: String, value: &impl Serialize) -> Result<Binding> {
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| invalid(e.to_string()))?;
        if bytes.len() > JSON_LIMIT {
            return Err(invalid("Generated JSON limit"));
        }
        self.add(path, Arc::new(bytes))
    }
}
impl<S: AssetSource> AssetSource for Session<'_, S> {
    fn read(&self, ns: &str, path: &str, maximum: usize) -> Result<Vec<u8>> {
        self.read_shared(ns, path, maximum)
            .map(|b| b.as_ref().clone())
    }
    fn read_shared(&self, ns: &str, path: &str, maximum: usize) -> Result<Arc<Vec<u8>>> {
        if ns == GENERATED {
            return self
                .generated
                .get(path)
                .filter(|b| b.len() <= maximum)
                .cloned()
                .ok_or_else(|| invalid("Missing/oversized generated input"));
        }
        // Reference paths in older manifests do not carry their own digest.
        // Cache all host bytes once, then each consumer verifies its binding.
        relative_path(path)?;
        let key = (ns.to_owned(), path.to_owned());
        let mut cache = self.cache.borrow_mut();
        if let Some(body) = cache.get(&key) {
            return if body.len() <= maximum {
                Ok(body.clone())
            } else {
                Err(invalid("Host input exceeds stage limit"))
            };
        }
        let body = self.source.read_shared(ns, path, maximum)?;
        if body.len() > maximum
            || cache
                .values()
                .map(|b| b.len())
                .sum::<usize>()
                .checked_add(body.len())
                .is_none_or(|n| n > 512 * 1024 * 1024)
        {
            return Err(invalid("Host inputs exceed stage or 512 MiB limit"));
        }
        cache.insert(key, body.clone());
        Ok(body)
    }
}
/// Always run fresh native fitting. Estimates are provisional; insufficient fits
/// preserve exact source timing. No review approval or library publication occurs.
pub fn prepare(
    request: &Request,
    source: &impl AssetSource,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<PreparedWork> {
    let mut event = |stage, completed, total| {
        control(Progress {
            stage,
            completed,
            total,
        })
    };
    event(Stage::Validate, 0, 1)?;
    for b in [&request.manifest, &request.motion]
        .into_iter()
        .chain(request.font.iter())
    {
        if b.namespace == GENERATED {
            return Err(invalid("Reserved generated namespace"));
        }
    }
    if !request.options.quality.is_finite() || !(0.0..=1.0).contains(&request.options.quality) {
        return Err(invalid("Invalid Vorbis quality"));
    }
    let mut session = Session {
        source,
        cache: RefCell::new(BTreeMap::new()),
        generated: BTreeMap::new(),
        bytes: 0,
    };
    let entries = {
        let mut inputs = Inputs::new(&session);
        let doc: Value = json(&inputs.checked(&request.manifest, JSON_LIMIT)?)?;
        let entries = work::entries(&doc, &request.manifest, &mut inputs)?;
        let reviews = crate::reviews::load(
            &mut inputs,
            &request.manifest.sha256,
            &entries,
            doc["sourceSwfSha256"].as_str(),
            &ReviewBindings {
                motion: Some(request.motion.clone()),
                ..Default::default()
            },
        )?;
        if reviews.motions.len() != entries.len() {
            return Err(invalid("Need measured motion for every chart"));
        }
        entries
    };
    let mut music = BTreeMap::new();
    for e in &entries {
        music
            .entry(e.music.sha256.clone())
            .or_insert(e.music.clone());
    }
    let mut supplied = Vec::new();
    let mut analysis = Vec::new();
    for (i, (hash, binding)) in music.iter().enumerate() {
        event(Stage::Audio, i, music.len())?;
        let media = audio::prepare_media(
            &audio::MediaRequest {
                manifest: request.manifest.clone(),
                audio: request.manifest.sibling(binding)?,
                codec: request.options.codec,
                decode: audio::DecodeOptions::default(),
                quality: request.options.quality,
            },
            &session,
            &mut || event(Stage::Audio, i, music.len()),
        )?;
        let prefix = format!("media/{hash}");
        let mut files = BTreeMap::new();
        // source.bin and source-manifest.json already exist as verified host inputs.
        for file in media
            .files
            .into_iter()
            .filter(|f| !["source.bin", "source-manifest.json"].contains(&f.path.as_str()))
        {
            files.insert(
                file.path.clone(),
                session.add(format!("{prefix}/{}", file.path), file.bytes)?,
            );
        }
        let receipt = session.json(format!("{prefix}/manifest.json"), &media.manifest)?;
        let pcm = files["pcm.wav"].clone();
        let audio = files[match request.options.codec {
            Codec::Wav => "music.wav",
            Codec::Ogg => "music.ogg",
        }]
        .clone();
        analysis.push(tempo::AnalysisPcm {
            source_audio_sha256: hash.clone(),
            pcm: pcm.clone(),
            receipt: receipt.clone(),
        });
        supplied.push(SuppliedMedia {
            source_audio_sha256: hash.clone(),
            audio,
            pcm,
            codec: request.options.codec,
            provenance: receipt,
        });
    }
    event(Stage::Tempo, 0, music.len())?;
    let tempo = tempo::prepare_tempo(
        &tempo::TempoRequest {
            manifest: request.manifest.clone(),
            media: analysis,
            legacy_analysis: None,
            cached: None,
        },
        &session,
        &mut || event(Stage::Tempo, 0, music.len()),
    )?;
    let grid = if tempo["status"] == "estimated" {
        Some(session.json("grid.json".into(), &tempo["review"])?)
    } else {
        None
    };
    let mut art_charts = serde_json::Map::new();
    if let Some(font) = &request.font {
        for (i, hash) in music.keys().enumerate() {
            event(Stage::Artwork, i, music.len())?;
            let group = entries
                .iter()
                .filter(|e| &e.music.sha256 == hash)
                .collect::<Vec<_>>();
            let first = group[0];
            let title = first
                .metadata
                .get("title")
                .or_else(|| first.metadata.get("musicTitle"))
                .and_then(Value::as_str)
                .unwrap_or(&first.chart.id);
            let artist = first
                .metadata
                .get("artist")
                .and_then(Value::as_str)
                .unwrap_or("");
            let art = artwork::title_cards(
                &artwork::TitleRequest {
                    title: title.into(),
                    artist: artist.into(),
                    source_manifest_sha256: request.manifest.sha256.clone(),
                    font: font.clone(),
                    roles: vec![artwork::Role::Banner],
                },
                &session,
            )?;
            let prefix = format!("artwork/{hash}");
            let banner = art.files.iter().find(|f| f.path == "banner.png").unwrap();
            let banner_binding =
                session.add(format!("{prefix}/banner.png"), banner.bytes.clone())?;
            let proof = session.json(format!("{prefix}/manifest.json"), &art.manifest)?;
            for e in group {
                art_charts.insert(e.chart.id.clone(), json!({"origin":"plain-title", "banner":{"path":banner_binding.path,"sha256":banner_binding.sha256},"evidence":{"path":proof.path,"sha256":proof.sha256}}));
            }
        }
    }
    let artwork = if art_charts.is_empty() {
        None
    } else {
        Some(session.json("artwork-review.json".into(), &json!({"schema":"ddi-reviewed-song-artwork/1", "sourceManifestSha256":request.manifest.sha256,"charts":art_charts}))?)
    };
    event(Stage::Songs, 0, entries.len())?;
    let mut options = request.options.work.clone();
    if tempo["status"] == "exact-source-fallback" {
        for e in &entries {
            if !e.metadata.contains_key("declaredDisplayBpm") {
                options
                    .display_bpms
                    .entry(e.chart.id.clone())
                    .or_insert_with(|| "*".into());
            }
        }
    }
    let work_request = WorkRequest {
        manifest: request.manifest.clone(),
        media: supplied,
        reviews: ReviewBindings {
            grid,
            motion: Some(request.motion.clone()),
            artwork,
            ratings: None,
        },
        options,
    };
    let mut result = work::prepare_work(&work_request, &session, &mut |p| {
        event(Stage::Songs, p.completed, p.total)
    })?;
    let report = json!({"schema":"ddi-danoni-fresh-preparation/1","request":request,
        "sourceManifestSha256":request.manifest.sha256,"tempo":tempo,
        "musicalQuality":"provisional; mathematical endpoint checks are not musical acceptance",
        "motionPolicy":"source-bound observed default motion; user speed multiplier remains multiplicative",
        "artworkPolicy":if request.font.is_some(){"generated plain title"}else{"absent; no host font supplied"},
        "ratingPolicy":"source stars only; unknown stays Edit/0","libraryInstalled":false});
    let bytes = Arc::new(serde_json::to_vec_pretty(&report).map_err(|e| invalid(e.to_string()))?);
    result.files.push(Artifact {
        path: "provenance/preparation.json".into(),
        bytes,
    });
    result.manifest["preparation"] = json!({"path":"provenance/preparation.json", "sha256":sha256(&result.files.last().unwrap().bytes), "tempoStatus":report["tempo"]["status"], "musicalQuality":"provisional"});
    result.manifest["artifacts"] = json!(
        result
            .files
            .iter()
            .map(|f| json!({"path":f.path,"sha256":sha256(&f.bytes),"bytes":f.bytes.len()}))
            .collect::<Vec<_>>()
    );
    event(Stage::Complete, 1, 1)?;
    Ok(result)
}
