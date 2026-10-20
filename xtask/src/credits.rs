//! `gen-credits`: the attribution the game must ship (CC-BY §3(a)).
//!
//! Reads every `assets/songs/<id>/PROVENANCE.toml`, the chart files (for
//! per-chart `#CREDIT` authors of playable charts), the bundled noteskins'
//! `assets/noteskins/<game>/<name>/PROVENANCE.toml`, `assets/fonts/`,
//! `video/FFMPEG.toml` (the video decoder module, LGPL) and
//! `THIRD_PARTY.toml` (code from other projects, with their notices), then
//! writes `CREDITS.md` at the repo root (checked in; CI verifies it is fresh)
//! and `target/credits/credits.json` (bundled, shown on the Credits screen).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ddi_chart::formats::sm::parse_simfile;
use serde::Serialize;

use crate::{noteskins, songs};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf()
}

#[derive(Debug, Serialize)]
struct CreditsJson {
    songs: Vec<SongCredit>,
    noteskins: Vec<SkinCredit>,
    fonts: Vec<FontCredit>,
    video: VideoCredit,
    code: Vec<CodeCredit>,
    /// The Rust libraries in the web build and the desktop binary, with
    /// their licences' notices (`rust_credits`; THIRD_PARTY_RUST.md).
    #[serde(skip)]
    rust_all: crate::rust_credits::RustCredits,
    /// One build's of them: the web's in `credits.json`, the desktop's in
    /// `credits-desktop.json`.
    rust: Option<crate::rust_credits::Section>,
    /// The published site (`seo/metadata.toml`), for CREDITS.md's links to
    /// files the site serves; the Credits screen links them relatively.
    #[serde(skip)]
    site_url: String,
}

#[derive(serde::Deserialize)]
struct SiteMetadata {
    canonical_url: String,
}

/// Code or data from another project compiled into the game
/// (`THIRD_PARTY.toml`), with the notice its licence asks to ship.
#[derive(Debug, Serialize)]
struct CodeCredit {
    name: String,
    what: String,
    files: Vec<String>,
    spdx: String,
    /// The licence's name as shown.
    license: String,
    holders: Vec<String>,
    source: String,
    commit: String,
    license_url: String,
    notice: String,
}

#[derive(serde::Deserialize)]
struct ThirdParty {
    #[serde(default)]
    code: Vec<ThirdPartyCode>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ThirdPartyCode {
    name: String,
    what: String,
    files: Vec<String>,
    spdx: String,
    holders: Vec<String>,
    source: String,
    commit: String,
    license_url: String,
    /// A file holding the notice; for MIT it may be omitted (the notice is
    /// `LICENSES/MIT.txt` with the holders' copyright line).
    #[serde(default)]
    notice: Option<String>,
}

fn code_credits(root: &Path) -> Result<Vec<CodeCredit>> {
    let path = root.join("THIRD_PARTY.toml");
    let tp: ThirdParty =
        toml::from_str(&fs::read_to_string(&path)?).context("parsing THIRD_PARTY.toml")?;
    let mut out = Vec::new();
    for c in tp.code {
        for f in &c.files {
            if !root.join(f).is_file() {
                bail!("THIRD_PARTY.toml: {}: {f} does not exist", c.name);
            }
        }
        let notice = match (&c.notice, c.spdx.as_str()) {
            (Some(file), _) => fs::read_to_string(root.join(file))
                .with_context(|| format!("THIRD_PARTY.toml: {}: reading {file}", c.name))?,
            (None, "MIT") => {
                let template = fs::read_to_string(root.join("LICENSES/MIT.txt"))?;
                let copyright = c
                    .holders
                    .iter()
                    .map(|h| format!("Copyright (c) {h}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !template.contains("Copyright (c) <year> <copyright holders>") {
                    bail!("LICENSES/MIT.txt: no copyright line to fill in");
                }
                template.replace("Copyright (c) <year> <copyright holders>", &copyright)
            }
            (None, other) => bail!(
                "THIRD_PARTY.toml: {}: a {other} entry needs a notice file",
                c.name
            ),
        };
        out.push(CodeCredit {
            name: c.name,
            what: c.what,
            files: c.files,
            license: license_name(&c.spdx),
            spdx: c.spdx,
            holders: c.holders,
            source: c.source,
            commit: c.commit,
            license_url: c.license_url,
            notice: notice.trim_end().to_string(),
        });
    }
    Ok(out)
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

/// A bundled noteskin (`assets/noteskins/<game>/<name>/`).
#[derive(Debug, Serialize)]
struct SkinCredit {
    name: String,
    author: String,
    source_url: String,
    source_path: String,
    source_commit: String,
    retrieved: String,
    parts: Vec<PartCredit>,
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

/// The video decoder module: FFmpeg and what is linked with it
/// (`video/build.sh` serves the licence texts next to the module).
#[derive(Debug, Serialize)]
struct VideoCredit {
    name: String,
    version: String,
    spdx: String,
    homepage: String,
    source_url: String,
    /// The release tarball at FFmpeg, and the same file served next to the
    /// module (`xtask stage-video`), as a path relative to the site.
    tarball_url: String,
    tarball: String,
    tarball_sha256: String,
    /// The flags after the toolchain ones, the decoders last.
    configure: Vec<String>,
    /// The desktop helper's (`video/build-native.sh`), the decoders last.
    native_configure: Vec<String>,
    components: Vec<ComponentCredit>,
    /// Served next to the module, when the build has it.
    notices: String,
}

#[derive(Debug, Serialize)]
struct ComponentCredit {
    name: String,
    spdx: String,
    url: String,
}

/// `video/FFMPEG.toml`: the fields the credits name (the build script
/// reads the rest).
#[derive(serde::Deserialize)]
struct FfmpegPin {
    name: String,
    version: String,
    license: String,
    homepage: String,
    source_url: String,
    tarball_url: String,
    tarball_sha256: String,
    zlib_version: String,
    wasi_sdk_version: String,
    wasi_libc_commit: String,
    configure: Vec<String>,
    native_configure: Vec<String>,
    video_decoders: Vec<String>,
}

fn ffmpeg_pin(root: &Path) -> Result<FfmpegPin> {
    toml::from_str(&fs::read_to_string(root.join("video/FFMPEG.toml"))?)
        .context("parsing video/FFMPEG.toml")
}

/// The file name of FFmpeg's release tarball, which `stage-video` serves
/// next to the module.
pub fn ffmpeg_tarball(root: &Path) -> Result<String> {
    let pin = ffmpeg_pin(root)?;
    match pin.tarball_url.rsplit('/').next() {
        Some(name) if !name.is_empty() => Ok(name.to_string()),
        _ => bail!("video/FFMPEG.toml: tarball_url has no file name"),
    }
}

fn video_credit(root: &Path) -> Result<VideoCredit> {
    let tarball = format!("video/{}", ffmpeg_tarball(root)?);
    let pin = ffmpeg_pin(root)?;
    let decoders = format!("--enable-decoder={}", pin.video_decoders.join(","));
    let mut configure = pin.configure;
    configure.push(decoders.clone());
    let mut native_configure = pin.native_configure;
    native_configure.push(decoders);
    let wasi_sdk = pin.wasi_sdk_version;
    Ok(VideoCredit {
        name: pin.name,
        version: pin.version,
        spdx: pin.license,
        homepage: pin.homepage,
        source_url: pin.source_url,
        tarball_url: pin.tarball_url,
        tarball,
        tarball_sha256: pin.tarball_sha256,
        configure,
        native_configure,
        components: vec![
            ComponentCredit {
                name: format!("zlib {}", pin.zlib_version),
                spdx: "Zlib".into(),
                url: "https://zlib.net/".into(),
            },
            ComponentCredit {
                name: format!(
                    "wasi-libc from wasi-sdk {wasi_sdk}, with parts from musl (MIT), cloudlibc (BSD-2-Clause) and dlmalloc (public domain)"
                ),
                spdx: "MIT".into(),
                url: format!(
                    "https://github.com/WebAssembly/wasi-libc/tree/{}",
                    pin.wasi_libc_commit
                ),
            },
            ComponentCredit {
                name: format!("LLVM compiler-rt builtins from wasi-sdk {wasi_sdk}"),
                spdx: "Apache-2.0 WITH LLVM-exception".into(),
                url: "https://github.com/WebAssembly/wasi-sdk".into(),
            },
        ],
        notices: "video/ddivideo.LICENSE.txt".into(),
    })
}

fn license_name(spdx: &str) -> String {
    match spdx {
        "CC-BY-3.0" => "CC BY 3.0".into(),
        "CC-BY-4.0" => "CC BY 4.0".into(),
        "CC-BY-SA-3.0" => "CC BY-SA 3.0".into(),
        "CC-BY-SA-4.0" => "CC BY-SA 4.0".into(),
        "CC0-1.0" => "CC0 1.0".into(),
        "OFL-1.1" => "SIL Open Font License 1.1".into(),
        "LGPL-2.1-or-later" => "GNU LGPL 2.1 or later".into(),
        "Zlib" => "zlib licence".into(),
        "MIT" => "MIT".into(),
        "Apache-2.0" => "Apache License 2.0".into(),
        "Unlicense" => "The Unlicense".into(),
        "Apache-2.0 WITH LLVM-exception" => "Apache 2.0 with LLVM Exceptions".into(),
        "LicenseRef-StepMania" => "StepMania's licence (MIT-style)".into(),
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
        "LGPL-2.1-or-later" => "https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html".into(),
        "Zlib" => "https://zlib.net/zlib_license.html".into(),
        "MIT" => "https://opensource.org/license/mit".into(),
        "Apache-2.0" => "https://www.apache.org/licenses/LICENSE-2.0".into(),
        "Unlicense" => "https://unlicense.org/".into(),
        "Apache-2.0 WITH LLVM-exception" => "https://llvm.org/LICENSE.txt".into(),
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

    let noteskins_out = noteskins::scan(root)?
        .into_iter()
        .filter(|(_, prov, _)| !prov.ours)
        .map(|(_, prov, folder)| SkinCredit {
            name: folder.name,
            author: prov.author,
            source_url: prov.source.url,
            source_path: prov.source.path,
            source_commit: prov.source.commit,
            retrieved: prov.source.retrieved,
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
            excluded: prov.excluded,
            modifications: prov.modifications,
        })
        .collect();

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
        noteskins: noteskins_out,
        fonts,
        video: video_credit(root)?,
        code: code_credits(root)?,
        rust_all: crate::rust_credits::collect(root)?,
        rust: None,
        site_url: {
            let meta: SiteMetadata =
                toml::from_str(&fs::read_to_string(root.join("seo/metadata.toml"))?)
                    .context("parsing seo/metadata.toml")?;
            meta.canonical_url
        },
    })
}

fn render_markdown(c: &CreditsJson) -> String {
    let mut md = String::new();
    md.push_str("# Credits\n\n");
    md.push_str("<!-- Generated by `cargo run -p xtask -- gen-credits` from assets/**/PROVENANCE.toml, video/FFMPEG.toml and THIRD_PARTY.toml. Do not edit by hand. -->\n\n");
    md.push_str("Dito Dito Infinito's code is MIT-licensed. The bundled demo songs, noteskins and fonts, the video decoder and the code from other projects below are third-party works used under their own licences; this file and the in-game Credits screen carry the attribution those licences require. Full licence texts are in `LICENSES/`.\n\n");
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
    if !c.noteskins.is_empty() {
        md.push_str("## Noteskins\n\n");
        for k in &c.noteskins {
            let _ = writeln!(md, "### {} — {}\n", k.name, k.author);
            let _ = writeln!(
                md,
                "Source: [{}]({}) at commit `{}`, folder `{}` (retrieved {}).\n",
                k.source_url, k.source_url, k.source_commit, k.source_path, k.retrieved
            );
            for p in &k.parts {
                let _ = writeln!(
                    md,
                    "- **{}**: {} — [{}]({})",
                    p.part,
                    p.holders.join(", "),
                    license_name(&p.spdx),
                    p.url
                );
            }
            if !k.excluded.is_empty() {
                let _ = writeln!(md, "- Not included: {}", k.excluded.join("; "));
            }
            if k.modifications.is_empty() {
                md.push_str("- Modifications: none (files are byte-identical to upstream)\n");
            } else {
                let _ = writeln!(md, "- Modifications: {}", k.modifications.join("; "));
            }
            md.push('\n');
        }
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
    let v = &c.video;
    md.push_str("## Video decoder\n\n");
    let _ = writeln!(
        md,
        "Background videos the browser cannot decode play through a separate module, `video/ddivideo.wasm`, fetched only when a song needs it. It uses code of [{} {}]({}) licensed under the [{}]({}); its source, unmodified, is served next to the module: [`{}`]({}{}) (SHA-256 `{}`; the [release]({}) at FFmpeg, tag: {}). It is built by `video/build.sh` with the configure flags below and linked with `video/ddivideo.c` and `video/packets.c` (this repository's code, MIT); the build script rebuilds it, also against a modified {}.\n",
        v.name,
        v.version,
        v.homepage,
        license_name(&v.spdx),
        license_url(&v.spdx),
        v.tarball,
        c.site_url,
        v.tarball,
        v.tarball_sha256,
        v.tarball_url,
        v.source_url,
        v.name
    );
    md.push_str("Also in the module:\n\n");
    for p in &v.components {
        let _ = writeln!(
            md,
            "- [{}]({}) — [{}]({})",
            p.name,
            p.url,
            license_name(&p.spdx),
            license_url(&p.spdx)
        );
    }
    let _ = writeln!(
        md,
        "\nThe licence texts are served next to the module (`{}`) in builds that include it.\n",
        v.notices
    );
    md.push_str(
        "Configure flags (after the toolchain's `--cc`, `--ar` and the zlib paths):\n\n```\n",
    );
    for flag in &v.configure {
        let _ = writeln!(md, "{flag}");
    }
    md.push_str("```\n");
    let _ = writeln!(
        md,
        "\nOn desktop the same {} release, with zlib, is linked statically into a separate program, `ddi-video-helper` (`video/helper`, MIT), which decodes in a sandboxed process of its own; the game never links FFmpeg. It is built by `video/build-native.sh` with the configure flags below and the desktop's packages ship it with its notice (`video/ddi-video-helper.LICENSE.txt`), FFmpeg's source and the helper's source, so it can be rebuilt against a modified {} (`cargo build -p ddi-video-helper`).\n",
        v.name, v.name
    );
    md.push_str("Configure flags of the desktop helper's FFmpeg (after `--prefix` and the toolchain's):\n\n```\n");
    for flag in &v.native_configure {
        let _ = writeln!(md, "{flag}");
    }
    md.push_str("```\n");
    if !c.code.is_empty() {
        md.push_str("\n## Code from other projects\n\n");
        for k in &c.code {
            let _ = writeln!(md, "### {}\n", k.name);
            let _ = writeln!(
                md,
                "{} Source: {} at commit `{}`; licence: [{}]({}). Files: {}.\n",
                k.what,
                k.source,
                k.commit,
                k.license,
                k.license_url,
                k.files
                    .iter()
                    .map(|f| format!("`{f}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let _ = writeln!(
                md,
                "<details><summary>Notice</summary>\n\n```text\n{}\n```\n\n</details>\n",
                k.notice
            );
        }
    }
    let r = &c.rust_all;
    let _ = writeln!(
        md,
        "\n## Rust libraries\n\nThe game is built on open-source Rust libraries (crates): {} in the web build (the app and the Opus worker) and {} in the desktop app (Linux, macOS and Windows). Their names, versions and licences, and the {} licence notices they ask to ship, are in [THIRD_PARTY_RUST.md](THIRD_PARTY_RUST.md) and on the Credits screen.",
        r.web.len(),
        r.desktop.len(),
        r.notices.len()
    );
    md
}

/// THIRD_PARTY_RUST.md: both builds' crates and every notice.
fn render_rust_markdown(r: &crate::rust_credits::RustCredits) -> String {
    let mut md = String::new();
    md.push_str("# Rust libraries\n\n");
    md.push_str("<!-- Generated by `cargo run -p xtask -- gen-credits` from `cargo tree` and `cargo metadata`. Do not edit by hand. -->\n\n");
    md.push_str("The Rust libraries (crates) compiled into Dito Dito Infinito, each with its licence as it declares it and the one it is used under where it offers a choice, then the licence notices they ask to ship, each text once (from the crate's own files; the SPDX text from `LICENSES/` where a crate ships none). Procedural macros, build scripts and development dependencies are not in the binaries and are not listed.\n\n");
    for (title, list) in [
        ("Web build (the app and the Opus worker, wasm32)", &r.web),
        ("Desktop app (Linux, macOS and Windows)", &r.desktop),
    ] {
        let _ = writeln!(md, "## {title}\n\n{} crates.\n", list.len());
        md.push_str(
            "| Crate | Version | Licence | Used under | Notices |\n|---|---|---|---|---|\n",
        );
        for k in list {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} |",
                k.name,
                k.version,
                k.license,
                k.chosen,
                k.notices
                    .iter()
                    .map(|i| format!("[{}](#notice-{})", i + 1, i + 1))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        md.push('\n');
    }
    md.push_str("## Notices\n\n");
    for (i, n) in r.notices.iter().enumerate() {
        let users: Vec<String> = r
            .web
            .iter()
            .chain(&r.desktop)
            .filter(|k| k.notices.contains(&i))
            .map(|k| format!("{} {}", k.name, k.version))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let _ = writeln!(
            md,
            "<a id=\"notice-{}\"></a>\n\n### {}. {} — {}\n\n```text\n{}\n```\n",
            i + 1,
            i + 1,
            n.license,
            users.join(", "),
            n.text.replace("```", "ʼʼʼ")
        );
    }
    md
}

pub(crate) fn gen_credits(check: bool) -> Result<()> {
    let root = repo_root();
    let mut credits = collect(&root)?;
    let md = render_markdown(&credits);
    let rust_md = render_rust_markdown(&credits.rust_all);
    for (name, text) in [("CREDITS.md", &md), ("THIRD_PARTY_RUST.md", &rust_md)] {
        let path = root.join(name);
        if check {
            let current = fs::read_to_string(&path).unwrap_or_default();
            if &current != text {
                bail!("{name} is stale; run `cargo run -p xtask -- gen-credits`");
            }
        } else {
            write_if_changed(&path, text)?;
        }
    }
    let out_dir = root.join("target/credits");
    fs::create_dir_all(&out_dir)?;
    // The web's file (served, fetched by the Credits page) carries the web
    // build's crates; the desktop's its own.
    credits.rust = Some(credits.rust_all.section(&credits.rust_all.web));
    let json = serde_json::to_string(&credits)? + "\n";
    write_if_changed(&out_dir.join("credits.json"), &json)?;
    credits.rust = Some(credits.rust_all.section(&credits.rust_all.desktop));
    let desktop = serde_json::to_string(&credits)? + "\n";
    write_if_changed(&out_dir.join("credits-desktop.json"), &desktop)?;
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
