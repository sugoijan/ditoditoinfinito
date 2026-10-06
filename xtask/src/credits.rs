//! `gen-credits`: the attribution the game must ship (CC-BY §3(a)).
//!
//! Reads every `assets/songs/<id>/PROVENANCE.toml`, the chart files (for
//! per-chart `#CREDIT` authors of playable charts) and `assets/fonts/`, then
//! writes `CREDITS.md` at the repo root (checked in; CI verifies it is fresh)
//! and `target/credits/credits.json` (bundled, shown on the Credits screen).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ddi_chart::formats::sm::parse_simfile;
use serde::Serialize;

use crate::songs;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf()
}

#[derive(Debug, Serialize)]
struct CreditsJson {
    songs: Vec<SongCredit>,
    fonts: Vec<FontCredit>,
}

#[derive(Debug, Serialize)]
struct SongCredit {
    id: String,
    title: String,
    artist: String,
    source_url: String,
    source_path: String,
    source_commit: String,
    retrieved: String,
    parts: Vec<PartCredit>,
    /// Chart author → playable charts credited to them.
    chart_authors: BTreeMap<String, Vec<String>>,
    excluded: Vec<String>,
    modifications: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PartCredit {
    part: String,
    spdx: String,
    holders: Vec<String>,
    url: String,
}

#[derive(Debug, Serialize)]
struct FontCredit {
    file: String,
    spdx: String,
    holders: String,
    source: String,
    modified: String,
}

fn license_name(spdx: &str) -> String {
    match spdx {
        "CC-BY-3.0" => "CC BY 3.0".into(),
        "CC-BY-4.0" => "CC BY 4.0".into(),
        "CC-BY-SA-3.0" => "CC BY-SA 3.0".into(),
        "CC-BY-SA-4.0" => "CC BY-SA 4.0".into(),
        "CC0-1.0" => "CC0 1.0".into(),
        "OFL-1.1" => "SIL Open Font License 1.1".into(),
        other => other.into(),
    }
}

fn license_url(spdx: &str) -> String {
    match spdx {
        "CC-BY-3.0" => "https://creativecommons.org/licenses/by/3.0/".into(),
        "CC-BY-4.0" => "https://creativecommons.org/licenses/by/4.0/".into(),
        "CC-BY-SA-3.0" => "https://creativecommons.org/licenses/by-sa/3.0/".into(),
        "CC-BY-SA-4.0" => "https://creativecommons.org/licenses/by-sa/4.0/".into(),
        "CC0-1.0" => "https://creativecommons.org/publicdomain/zero/1.0/".into(),
        "OFL-1.1" => "https://openfontlicense.org/".into(),
        _ => String::new(),
    }
}

fn collect(root: &Path) -> Result<CreditsJson> {
    let mut songs_out = Vec::new();
    for (id, prov, chart_file) in songs::scan(root)? {
        let dir = root.join("assets/songs").join(&id);
        let text = fs::read_to_string(dir.join(&chart_file))
            .with_context(|| format!("reading {chart_file}"))?;
        let ext = chart_file.rsplit('.').next().unwrap_or("sm");
        let song =
            parse_simfile(&text, ext).map_err(|e| anyhow::anyhow!("{id}/{chart_file}: {e}"))?;
        let mut chart_authors: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for chart in song.playable_charts() {
            let author = if chart.credit.trim().is_empty() {
                "(uncredited)".to_string()
            } else {
                chart.credit.trim().to_string()
            };
            chart_authors.entry(author).or_default().push(format!(
                "{} {:?} {}",
                chart.layout, chart.difficulty, chart.meter
            ));
        }
        if prov.licenses.is_empty() {
            bail!("{id}: PROVENANCE.toml has no [[licenses]] entries");
        }
        songs_out.push(SongCredit {
            id: id.clone(),
            title: prov.title.clone(),
            artist: prov.artist.clone(),
            source_url: prov.source.url.clone(),
            source_path: prov.source.path.clone(),
            source_commit: prov.source.commit.clone(),
            retrieved: prov.source.retrieved.clone(),
            parts: prov
                .licenses
                .iter()
                .map(|l| PartCredit {
                    part: l.part.clone(),
                    spdx: l.spdx.clone(),
                    holders: l.holders.clone(),
                    url: if l.url.is_empty() {
                        license_url(&l.spdx)
                    } else {
                        l.url.clone()
                    },
                })
                .collect(),
            chart_authors,
            excluded: prov.excluded.clone(),
            modifications: prov.modifications.clone(),
        });
    }

    // Fonts: assets/fonts/PROVENANCE.toml with [[file]] entries.
    let mut fonts = Vec::new();
    let font_prov = root.join("assets/fonts/PROVENANCE.toml");
    if font_prov.is_file() {
        #[derive(serde::Deserialize)]
        struct FontProv {
            #[serde(default)]
            file: Vec<FontFile>,
        }
        #[derive(serde::Deserialize)]
        struct FontFile {
            path: String,
            license: String,
            source: String,
            #[serde(default)]
            modified: String,
            #[serde(default)]
            holders: String,
        }
        let fp: FontProv = toml::from_str(&fs::read_to_string(&font_prov)?)
            .context("parsing assets/fonts/PROVENANCE.toml")?;
        for f in fp.file {
            fonts.push(FontCredit {
                file: f.path,
                spdx: f.license,
                holders: f.holders,
                source: f.source,
                modified: f.modified,
            });
        }
    }
    Ok(CreditsJson {
        songs: songs_out,
        fonts,
    })
}

fn render_markdown(c: &CreditsJson) -> String {
    let mut md = String::new();
    md.push_str("# Credits\n\n");
    md.push_str("<!-- Generated by `cargo run -p xtask -- gen-credits` from assets/**/PROVENANCE.toml and REUSE.toml. Do not edit by hand. -->\n\n");
    md.push_str("Dito Dito Infinito's code is MIT-licensed. The bundled demo songs and fonts below are third-party works used under their own licences; this file and the in-game Credits screen carry the attribution those licences require. Full licence texts are in `LICENSES/`.\n\n");
    md.push_str("## Songs\n\n");
    for s in &c.songs {
        let _ = writeln!(md, "### {} — {}\n", s.title, s.artist);
        let _ = writeln!(
            md,
            "Source: [{}]({}) at commit `{}`, folder `{}` (retrieved {}).\n",
            s.source_url, s.source_url, s.source_commit, s.source_path, s.retrieved
        );
        for p in &s.parts {
            let _ = writeln!(
                md,
                "- **{}**: {} — [{}]({})",
                p.part,
                p.holders.join(", "),
                license_name(&p.spdx),
                p.url
            );
        }
        if !s.chart_authors.is_empty() {
            md.push_str("- **chart authors** (from the simfile's `#CREDIT` tags):\n");
            for (author, charts) in &s.chart_authors {
                let _ = writeln!(md, "  - {}: {}", author, charts.join(", "));
            }
        }
        if !s.excluded.is_empty() {
            let _ = writeln!(md, "- Not included: {}", s.excluded.join("; "));
        }
        if s.modifications.is_empty() {
            md.push_str("- Modifications: none (files are byte-identical to upstream)\n");
        } else {
            let _ = writeln!(md, "- Modifications: {}", s.modifications.join("; "));
        }
        md.push('\n');
    }
    if !c.fonts.is_empty() {
        md.push_str("## Fonts\n\n");
        for f in &c.fonts {
            let _ = writeln!(
                md,
                "- `{}`: {} — [{}]({}); source: {}{}",
                f.file,
                f.holders,
                license_name(&f.spdx),
                license_url(&f.spdx),
                f.source,
                if f.modified.is_empty() {
                    String::new()
                } else {
                    format!("; modified: {}", f.modified)
                }
            );
        }
        md.push('\n');
    }
    md
}

pub(crate) fn gen_credits(check: bool) -> Result<()> {
    let root = repo_root();
    let credits = collect(&root)?;
    let md = render_markdown(&credits);
    let json = serde_json::to_string_pretty(&credits)? + "\n";
    let md_path = root.join("CREDITS.md");
    if check {
        let current = fs::read_to_string(&md_path).unwrap_or_default();
        if current != md {
            bail!("CREDITS.md is stale; run `cargo run -p xtask -- gen-credits`");
        }
    } else {
        write_if_changed(&md_path, &md)?;
    }
    let out_dir = root.join("target/credits");
    fs::create_dir_all(&out_dir)?;
    write_if_changed(&out_dir.join("credits.json"), &json)?;
    if !check {
        println!(
            "wrote CREDITS.md and target/credits/credits.json ({} songs, {} fonts)",
            credits.songs.len(),
            credits.fonts.len()
        );
    }
    Ok(())
}

/// Avoids touching the file (and its mtime) when the content is identical, so
/// file watchers such as Trunk's do not see a change on every build.
fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if fs::read_to_string(path)
        .map(|c| c == content)
        .unwrap_or(false)
    {
        return Ok(());
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}
