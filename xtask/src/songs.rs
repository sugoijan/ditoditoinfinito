//! `gen-songs`: scan `assets/songs/<id>/` and emit `target/songs/index.json`,
//! the manifest the app fetches at runtime. Each song directory must contain
//! exactly one `.ssc`/`.sm` and a `PROVENANCE.toml` naming the audio file.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ddi_chart::formats::sm::parse_simfile;
use serde::{Deserialize, Serialize};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf()
}

/// `assets/songs/<id>/PROVENANCE.toml`.
#[derive(Debug, Deserialize)]
pub(crate) struct Provenance {
    pub(crate) title: String,
    pub(crate) artist: String,
    /// Audio file name inside the directory.
    pub(crate) music: String,
    #[serde(default)]
    pub(crate) banner: Option<String>,
    #[serde(default)]
    pub(crate) background: Option<String>,
    /// One-line credit for the song list, e.g. "Sevish · CC BY 4.0".
    #[serde(default)]
    pub(crate) credit: String,
    pub(crate) source: Source,
    #[serde(default)]
    pub(crate) licenses: Vec<LicenseEntry>,
    #[serde(default)]
    pub(crate) excluded: Vec<String>,
    #[serde(default)]
    pub(crate) modifications: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) commit: String,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) retrieved: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LicenseEntry {
    /// `music`, `charts`, `graphics` or a file name.
    pub(crate) part: String,
    pub(crate) spdx: String,
    pub(crate) holders: Vec<String>,
    #[serde(default)]
    pub(crate) url: String,
}

#[derive(Debug, Serialize, PartialEq)]
struct ManifestEntry {
    id: String,
    title: String,
    artist: String,
    chart: String,
    music: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<String>,
    credit: String,
    bpm: String,
    preview_start: f64,
    preview_length: f64,
    /// Playable charts, in file order; `index` is the position in `Song::charts`.
    charts: Vec<ChartInfo>,
}

#[derive(Debug, Serialize, PartialEq)]
struct ChartInfo {
    index: usize,
    layout: String,
    difficulty: String,
    meter: u32,
    name: String,
    credit: String,
    notes: usize,
}

#[derive(Debug, Serialize, PartialEq)]
struct Manifest {
    songs: Vec<ManifestEntry>,
}

pub(crate) fn scan(root: &Path) -> Result<Vec<(String, Provenance, String)>> {
    let dir = root.join("assets/songs");
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut entries: Vec<_> = fs::read_dir(&dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let prov_path = path.join("PROVENANCE.toml");
        let prov: Provenance = toml::from_str(
            &fs::read_to_string(&prov_path)
                .with_context(|| format!("reading {}", prov_path.display()))?,
        )
        .with_context(|| format!("parsing {}", prov_path.display()))?;
        let mut charts: Vec<String> = fs::read_dir(&path)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".ssc") || n.ends_with(".sm"))
            .collect();
        charts.sort();
        // Prefer .ssc when both exist.
        let chart = charts
            .iter()
            .find(|n| n.ends_with(".ssc"))
            .or(charts.first())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("{id}: no .ssc/.sm chart file"))?;
        if !path.join(&prov.music).is_file() {
            bail!("{id}: music file `{}` missing", prov.music);
        }
        for f in [&prov.banner, &prov.background].into_iter().flatten() {
            if !path.join(f).is_file() {
                bail!("{id}: graphic `{f}` missing");
            }
        }
        out.push((id, prov, chart));
    }
    Ok(out)
}

pub(crate) fn gen_songs(check: bool) -> Result<()> {
    let root = repo_root();
    let songs = scan(&root)?;
    let mut entries = Vec::new();
    for (id, p, chart) in &songs {
        let text = fs::read_to_string(root.join("assets/songs").join(id).join(chart))?;
        let ext = chart.rsplit('.').next().unwrap_or("sm");
        let song = parse_simfile(&text, ext).map_err(|e| anyhow::anyhow!("{id}/{chart}: {e}"))?;
        let (lo, hi) = song.timing.bpm_range();
        let bpm = if (lo - hi).abs() < 0.01 {
            format!("{}", lo.round() as i64)
        } else {
            format!("{}-{}", lo.round() as i64, hi.round() as i64)
        };
        let mut indexed: Vec<_> = song
            .charts
            .iter()
            .enumerate()
            .filter(|(_, c)| ddi_chart::Layout::builtin(&c.layout).is_some())
            .collect();
        // Layout, then difficulty slot, then meter (file order is arbitrary).
        indexed.sort_by(|(_, a), (_, b)| {
            a.layout
                .cmp(&b.layout)
                .then(a.difficulty.cmp(&b.difficulty))
                .then(a.meter.cmp(&b.meter))
        });
        let charts = indexed
            .into_iter()
            .map(|(index, c)| ChartInfo {
                index,
                layout: c.layout.clone(),
                difficulty: format!("{:?}", c.difficulty),
                meter: c.meter,
                name: c.name.clone(),
                credit: c.credit.clone(),
                notes: c.judged_note_count(),
            })
            .collect();
        entries.push(ManifestEntry {
            id: id.clone(),
            title: p.title.clone(),
            artist: p.artist.clone(),
            chart: chart.clone(),
            music: p.music.clone(),
            banner: p.banner.clone(),
            background: p.background.clone(),
            credit: p.credit.clone(),
            bpm,
            preview_start: song.preview_start,
            preview_length: song.preview_length,
            charts,
        });
    }
    let manifest = Manifest { songs: entries };
    let json = serde_json::to_string_pretty(&manifest)? + "\n";
    let out_dir = root.join("target/songs");
    let out = out_dir.join("index.json");
    if check {
        let current = fs::read_to_string(&out).unwrap_or_default();
        if current != json {
            bail!(
                "{} is stale; run `cargo run -p xtask -- gen-songs`",
                out.display()
            );
        }
        return Ok(());
    }
    fs::create_dir_all(&out_dir)?;
    if fs::read_to_string(&out).map(|c| c == json).unwrap_or(false) {
        return Ok(());
    }
    fs::write(&out, json)?;
    println!("wrote {} ({} songs)", out.display(), manifest.songs.len());
    Ok(())
}
