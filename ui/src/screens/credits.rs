//! The credits (`docs/plans/ui-core.md`, step 6.4): `credits/credits.json`
//! (written by `xtask gen-credits`, the attribution the bundled works'
//! licences require) as a [`PageDoc`]: the songs, the noteskins, the
//! fonts, the video decoder (FFmpeg, from `video/FFMPEG.toml`, with its
//! licence texts) and the code from other projects, every source a link.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::page::{Block, PageDoc};

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Credits {
    pub songs: Vec<SongCredit>,
    #[serde(default)]
    pub noteskins: Vec<SkinCredit>,
    pub fonts: Vec<FontCredit>,
    pub video: VideoCredit,
    #[serde(default)]
    pub code: Vec<CodeCredit>,
    /// Whose wording (the shell sets it: the file is the same).
    #[serde(skip)]
    pub platform: super::data::Platform,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CodeCredit {
    pub name: String,
    pub what: String,
    pub license: String,
    pub holders: Vec<String>,
    pub source: String,
    pub commit: String,
    pub license_url: String,
    pub notice: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SongCredit {
    pub title: String,
    pub artist: String,
    pub source_url: String,
    pub source_path: String,
    pub source_commit: String,
    #[serde(default)]
    pub retrieved: String,
    pub parts: Vec<PartCredit>,
    #[serde(default)]
    pub chart_authors: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub excluded: Vec<String>,
    #[serde(default)]
    pub modifications: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SkinCredit {
    pub name: String,
    pub author: String,
    pub source_url: String,
    pub source_path: String,
    pub source_commit: String,
    #[serde(default)]
    pub retrieved: String,
    pub parts: Vec<PartCredit>,
    #[serde(default)]
    pub excluded: Vec<String>,
    #[serde(default)]
    pub modifications: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PartCredit {
    pub part: String,
    pub spdx: String,
    pub holders: Vec<String>,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct VideoCredit {
    pub name: String,
    pub version: String,
    pub spdx: String,
    pub homepage: String,
    pub source_url: String,
    pub tarball_url: String,
    pub tarball: String,
    pub tarball_sha256: String,
    pub configure: Vec<String>,
    pub components: Vec<ComponentCredit>,
    pub notices: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ComponentCredit {
    pub name: String,
    pub spdx: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FontCredit {
    pub file: String,
    pub spdx: String,
    pub holders: String,
    pub source: String,
    #[serde(default)]
    pub modified: String,
}

pub fn parse(json: &str) -> Result<Credits, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

/// A commit's first ten characters (whatever it holds: never cut inside
/// a character).
fn short(commit: &str) -> &str {
    commit
        .char_indices()
        .nth(10)
        .map_or(commit, |(i, _)| &commit[..i])
}

fn source(url: &str, path: &str, commit: &str, retrieved: &str) -> Vec<Block> {
    vec![
        Block::Link {
            label: format!("Source: {url}"),
            url: url.into(),
        },
        Block::Muted(format!(
            "commit {}, folder “{path}”{}",
            short(commit),
            if retrieved.is_empty() {
                String::new()
            } else {
                format!(", retrieved {retrieved}")
            }
        )),
    ]
}

fn parts(parts: &[PartCredit]) -> Vec<Block> {
    parts
        .iter()
        .map(|p| Block::Link {
            label: format!("{}: {} — {}", p.part, p.holders.join(", "), p.spdx),
            url: p.url.clone(),
        })
        .collect()
}

fn excluded_and_modified(excluded: &[String], modifications: &[String]) -> Vec<Block> {
    let mut v = Vec::new();
    if !excluded.is_empty() {
        v.push(Block::Muted(format!(
            "not included: {}",
            excluded.join("; ")
        )));
    }
    v.push(Block::Muted(if modifications.is_empty() {
        "modifications: none".into()
    } else {
        format!("modifications: {}", modifications.join("; "))
    }));
    v
}

/// The page: what the Yew credits showed, in the same order.
pub fn doc(c: &Credits) -> PageDoc {
    let mut b = vec![Block::Text(
        "Dito Dito Infinito's code is MIT-licensed. The bundled demo songs, noteskins and \
         fonts, the video decoder and the code from other projects are third-party works used \
         under their own licences, credited here as those licences require."
            .into(),
    )];
    b.push(Block::Heading("Songs".into()));
    for s in &c.songs {
        b.push(Block::Subheading(format!("{} — {}", s.title, s.artist)));
        b.extend(source(
            &s.source_url,
            &s.source_path,
            &s.source_commit,
            &s.retrieved,
        ));
        b.extend(parts(&s.parts));
        if !s.chart_authors.is_empty() {
            let authors: Vec<String> = s
                .chart_authors
                .iter()
                .map(|(a, charts)| format!("{a} ({})", charts.join(", ")))
                .collect();
            b.push(Block::Text(format!("chart authors: {}", authors.join(" "))));
        }
        b.extend(excluded_and_modified(&s.excluded, &s.modifications));
    }
    if !c.noteskins.is_empty() {
        b.push(Block::Heading("Noteskins".into()));
        for k in &c.noteskins {
            b.push(Block::Subheading(format!("{} — {}", k.name, k.author)));
            b.extend(source(
                &k.source_url,
                &k.source_path,
                &k.source_commit,
                &k.retrieved,
            ));
            b.extend(parts(&k.parts));
            b.extend(excluded_and_modified(&k.excluded, &k.modifications));
        }
    }
    b.push(Block::Heading("Fonts".into()));
    for f in &c.fonts {
        b.push(Block::Text(format!(
            "{}: {} — {}",
            f.file, f.holders, f.spdx
        )));
        b.push(Block::Link {
            label: "source".into(),
            url: f.source.clone(),
        });
        if !f.modified.is_empty() {
            b.push(Block::Muted(format!("modified: {}", f.modified)));
        }
    }
    let v = &c.video;
    let tarball = v.tarball.rsplit('/').next().unwrap_or(&v.tarball);
    b.push(Block::Heading("Video decoder".into()));
    let desktop = c.platform == super::data::Platform::Desktop;
    b.push(Block::Text(if desktop {
        format!(
            "Background videos play through a separate module, run in a sandbox of its own \
             when a song needs it. It uses code of {} {} licensed under the {}; its source, \
             unmodified, comes with the module (SHA-256 {}). It is built by video/build.sh in \
             this game's repository with the flags below and linked with the game's own code \
             (MIT).",
            v.name, v.version, v.spdx, v.tarball_sha256
        )
    } else {
        format!(
            "Background videos the browser cannot decode play through a separate module, \
             fetched only when a song needs it. It uses code of {} {} licensed under the {}; \
             its source, unmodified, is served next to the module (SHA-256 {}). It is built by \
             video/build.sh in this game's repository with the flags below and linked with the \
             game's own code (MIT).",
            v.name, v.version, v.spdx, v.tarball_sha256
        )
    }));
    b.push(Block::Link {
        label: format!("{} {}", v.name, v.version),
        url: v.homepage.clone(),
    });
    b.push(Block::Link {
        label: format!("Its source: {tarball}"),
        url: v.tarball.clone(),
    });
    b.push(Block::Link {
        label: "The release at FFmpeg".into(),
        url: v.tarball_url.clone(),
    });
    b.push(Block::Link {
        label: "The tag".into(),
        url: v.source_url.clone(),
    });
    for comp in &v.components {
        b.push(Block::Link {
            label: format!("{} — {}", comp.name, comp.spdx),
            url: comp.url.clone(),
        });
    }
    b.push(Block::Link {
        label: "Licence texts".into(),
        url: v.notices.clone(),
    });
    b.push(Block::Muted(if desktop {
        "(next to the module, in builds that include it)".into()
    } else {
        "(served with the module, in builds that include it)".into()
    }));
    b.push(Block::Fold {
        id: "configure".into(),
        label: "Configure flags".into(),
        body: vec![Block::Mono(v.configure.join("\n"))],
    });
    if !c.code.is_empty() {
        b.push(Block::Heading("Code from other projects".into()));
        for (i, k) in c.code.iter().enumerate() {
            b.push(Block::Subheading(k.name.clone()));
            b.push(Block::Text(k.what.clone()));
            b.push(Block::Muted(format!(
                "{} · commit {}",
                k.holders.join("; "),
                short(&k.commit)
            )));
            b.push(Block::Link {
                label: "source".into(),
                url: k.source.clone(),
            });
            b.push(Block::Link {
                label: k.license.clone(),
                url: k.license_url.clone(),
            });
            b.push(Block::Fold {
                id: format!("notice{i}"),
                label: "Notice".into(),
                body: vec![Block::Mono(k.notice.clone())],
            });
        }
    }
    b.push(Block::Gap);
    PageDoc {
        title: "Credits".into(),
        blocks: b,
    }
}

/// While the file loads, or when it could not be read.
pub fn waiting(error: Option<&str>) -> PageDoc {
    PageDoc {
        title: "Credits".into(),
        blocks: vec![match error {
            None => Block::Muted("loading…".into()),
            Some(e) => Block::Text(format!("The credits could not be read: {e}")),
        }],
    }
}

/// A small sample (the developer routes, tests).
pub fn sample() -> Credits {
    parse(SAMPLE).expect("the sample parses")
}

const SAMPLE: &str = r#"{
  "songs": [{
    "title": "Beatucada", "artist": "Kurio Prokos",
    "source_url": "https://example.org/songs", "source_path": "Beatucada",
    "source_commit": "0123456789abcdef", "retrieved": "2026-10-01",
    "parts": [
      {"part": "music", "spdx": "CC-BY-4.0", "holders": ["Kurio Prokos"], "url": "https://creativecommons.org/licenses/by/4.0/"},
      {"part": "charts", "spdx": "CC-BY-4.0", "holders": ["Someone With A Rather Long Name", "Another"], "url": "https://creativecommons.org/licenses/by/4.0/"}
    ],
    "chart_authors": {"Daniel Rotwind": ["Beginner", "Easy"]},
    "excluded": [], "modifications": ["banner resized: magick banner.png -resize 50% banner.png"]
  }],
  "noteskins": [{
    "name": "Delta", "author": "StepMania", "source_url": "https://example.org/skins",
    "source_path": "NoteSkins/dance/delta", "source_commit": "fedcba9876543210",
    "parts": [{"part": "art", "spdx": "MIT", "holders": ["StepMania contributors"], "url": "https://opensource.org/license/mit"}]
  }],
  "fonts": [{"file": "IosevkaTerm-Regular.woff2", "spdx": "OFL-1.1", "holders": "Renzhi Li", "source": "https://github.com/be5invis/Iosevka", "modified": "subset"}],
  "video": {
    "name": "FFmpeg", "version": "8.0", "spdx": "LGPL-2.1-or-later",
    "homepage": "https://ffmpeg.org", "source_url": "https://example.org/ffmpeg/tag",
    "tarball_url": "https://ffmpeg.org/releases/ffmpeg-8.0.tar.xz",
    "tarball": "video/ffmpeg-8.0.tar.xz",
    "tarball_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
    "configure": ["--disable-everything", "--enable-decoder=h264", "--enable-small"],
    "components": [{"name": "libavcodec", "spdx": "LGPL-2.1-or-later", "url": "https://ffmpeg.org/libavcodec.html"}],
    "notices": "video/ddivideo.LICENSE.txt"
  },
  "code": [{
    "name": "StepMania noteskin metrics", "what": "Metrics of the default noteskins.",
    "license": "MIT", "holders": ["Chris Danford", "Glenn Maynard"],
    "source": "https://github.com/stepmania/stepmania", "commit": "abcdef0123456789",
    "license_url": "https://github.com/stepmania/stepmania/blob/master/Docs/Licenses.txt",
    "notice": "Copyright (c) 2001-2015 Chris Danford, Glenn Maynard and others.\nPermission is hereby granted…"
  }]
}"#;
