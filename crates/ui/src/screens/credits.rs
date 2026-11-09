//! The credits (`docs/plans/ui-core.md`, step 6.4): `credits/credits.json`
//! (written by `xtask gen-credits`, the attribution the bundled works'
//! licences require) as a [`PageDoc`]: the songs, the noteskins, our
//! announcers (what made them, what their voices learned from), the
//! fonts, the video decoder (FFmpeg, from `video/FFMPEG.toml`, with its
//! licence texts), the code from other projects, every source a link, and
//! the Rust libraries of the build (folded: the web's in `credits.json`,
//! the desktop's in `credits-desktop.json`), with their notices.

use ddi_l10n::UiText;
use ddi_l10n::msg::pages as pm;
use serde::Deserialize;

pub use ddi_library::credits::{
    AnnouncerCredit, CodeCredit, ComponentCredit, FontCredit, PartCredit, SkinCredit, SongCredit,
    TrainedOnCredit, VideoCredit,
};

use super::page::{Block, PageDoc, PageTitle};

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Credits {
    pub songs: Vec<SongCredit>,
    #[serde(default)]
    pub noteskins: Vec<SkinCredit>,
    #[serde(default)]
    pub announcers: Vec<AnnouncerCredit>,
    pub fonts: Vec<FontCredit>,
    pub video: VideoCredit,
    #[serde(default)]
    pub code: Vec<CodeCredit>,
    /// The build's Rust libraries and their licences' notices.
    #[serde(default)]
    pub rust: Option<RustCredits>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RustCredits {
    pub crates: Vec<RustCrate>,
    pub notices: Vec<RustNotice>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RustCrate {
    pub name: String,
    pub version: String,
    /// As the crate declares it.
    pub license: String,
    /// What it is used under.
    pub chosen: String,
    /// Indices into [`RustCredits::notices`].
    pub notices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RustNotice {
    pub license: String,
    pub text: String,
    /// `name version` of the crates it covers.
    pub crates: Vec<String>,
}

/// The game's own source.
pub const SOURCE: &str = "https://github.com/sugoijan/ditoditoinfinito";

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

/// What is not ours to translate (a title, an author, a license's name,
/// a notice), as the page shows it.
fn data(text: impl AsRef<str>) -> String {
    UiText::data(text).into()
}

fn source(url: &str, path: &str, commit: &str, retrieved: &str) -> Vec<Block> {
    vec![
        Block::Link {
            label: pm::credits_source(url).into(),
            url: url.into(),
        },
        Block::Muted(
            if retrieved.is_empty() {
                pm::credits_commit_folder(short(commit), path)
            } else {
                pm::credits_commit_folder_retrieved(short(commit), path, retrieved)
            }
            .into(),
        ),
    ]
}

fn parts(parts: &[PartCredit]) -> Vec<Block> {
    parts
        .iter()
        .map(|p| Block::Link {
            label: data(format!("{}: {} — {}", p.part, p.holders.join(", "), p.spdx)),
            url: p.url.clone(),
        })
        .collect()
}

fn excluded_and_modified(excluded: &[String], modifications: &[String]) -> Vec<Block> {
    let mut v = Vec::new();
    if !excluded.is_empty() {
        v.push(Block::Muted(
            pm::credits_not_included(excluded.join("; ")).into(),
        ));
    }
    v.push(Block::Muted(
        if modifications.is_empty() {
            pm::credits_no_modifications()
        } else {
            pm::credits_modifications(modifications.join("; "))
        }
        .into(),
    ));
    v
}

/// The page: what the Yew credits showed, in the same order.
pub fn doc(c: &Credits, platform: super::data::Platform) -> PageDoc {
    let mut b = vec![Block::Text(pm::credits_about().into())];
    b.push(Block::Link {
        label: pm::credits_source(SOURCE).into(),
        url: SOURCE.into(),
    });
    b.push(Block::Heading(pm::credits_songs().into()));
    for s in &c.songs {
        b.push(Block::Subheading(data(format!(
            "{} — {}",
            s.title, s.artist
        ))));
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
            b.push(Block::Text(
                pm::credits_chart_authors(authors.join(" ")).into(),
            ));
        }
        b.extend(excluded_and_modified(&s.excluded, &s.modifications));
    }
    if !c.noteskins.is_empty() {
        b.push(Block::Heading(pm::credits_noteskins().into()));
        for k in &c.noteskins {
            b.push(Block::Subheading(data(format!(
                "{} — {}",
                k.name, k.author
            ))));
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
    if !c.announcers.is_empty() {
        b.push(Block::Heading(pm::credits_announcers().into()));
        for a in &c.announcers {
            b.push(Block::Subheading(data(format!(
                "{} — {}",
                a.name, a.author
            ))));
            b.push(Block::Text(data(format!(
                "{}: {}",
                a.language, a.made_with
            ))));
            b.push(Block::Link {
                label: pm::credits_model(&a.model_url).into(),
                url: a.model_url.clone(),
            });
            b.push(Block::Muted(
                if a.made.is_empty() {
                    pm::credits_revision(short(&a.revision))
                } else {
                    pm::credits_revision_made(short(&a.revision), &a.made)
                }
                .into(),
            ));
            b.extend(parts(&a.parts));
            for t in &a.trained_on {
                b.push(Block::Link {
                    label: pm::credits_learned_from(
                        &t.voice,
                        &t.what,
                        t.holders.join(", "),
                        &t.spdx,
                    )
                    .into(),
                    url: t.url.clone(),
                });
            }
        }
    }
    b.push(Block::Heading(pm::credits_fonts().into()));
    for f in &c.fonts {
        b.push(Block::Text(data(format!(
            "{}: {} — {}",
            f.file, f.holders, f.spdx
        ))));
        b.push(Block::Link {
            label: pm::credits_source_link().into(),
            url: f.source.clone(),
        });
        if !f.modified.is_empty() {
            b.push(Block::Muted(pm::credits_modified(&f.modified).into()));
        }
    }
    let v = &c.video;
    let tarball = v.tarball.rsplit('/').next().unwrap_or(&v.tarball);
    let platform = platform.l10n();
    b.push(Block::Heading(pm::credits_video().into()));
    b.push(Block::Text(
        pm::credits_video_about(platform, &v.name, &v.version, &v.spdx, &v.tarball_sha256).into(),
    ));
    b.push(Block::Link {
        label: data(format!("{} {}", v.name, v.version)),
        url: v.homepage.clone(),
    });
    b.push(Block::Link {
        label: pm::credits_video_source(tarball).into(),
        url: v.tarball.clone(),
    });
    b.push(Block::Link {
        label: pm::credits_video_release().into(),
        url: v.tarball_url.clone(),
    });
    b.push(Block::Link {
        label: pm::credits_video_tag().into(),
        url: v.source_url.clone(),
    });
    for comp in &v.components {
        b.push(Block::Link {
            label: data(format!("{} — {}", comp.name, comp.spdx)),
            url: comp.url.clone(),
        });
    }
    b.push(Block::Link {
        label: pm::credits_video_licenses().into(),
        url: v.notices.clone(),
    });
    b.push(Block::Muted(
        pm::credits_video_licenses_where(platform).into(),
    ));
    b.push(Block::Fold {
        id: "configure".into(),
        label: pm::credits_configure().into(),
        body: vec![Block::Mono(v.configure.join("\n"))],
    });
    if !c.code.is_empty() {
        b.push(Block::Heading(pm::credits_code().into()));
        for (i, k) in c.code.iter().enumerate() {
            b.push(Block::Subheading(data(&k.name)));
            b.push(Block::Text(data(&k.what)));
            b.push(Block::Muted(
                pm::credits_holders_commit(k.holders.join("; "), short(&k.commit)).into(),
            ));
            b.push(Block::Link {
                label: pm::credits_source_link().into(),
                url: k.source.clone(),
            });
            b.push(Block::Link {
                label: data(&k.license),
                url: k.license_url.clone(),
            });
            b.push(Block::Fold {
                id: format!("notice{i}"),
                label: pm::credits_notice().into(),
                body: vec![Block::Mono(k.notice.clone())],
            });
        }
    }
    if let Some(r) = &c.rust {
        b.extend(rust(r, platform));
    }
    b.push(Block::Gap);
    PageDoc {
        title: title(),
        blocks: b,
    }
}

/// The page's title.
fn title() -> PageTitle {
    PageTitle::new(pm::credits_title(), pm::credits_heading())
}

/// The Rust libraries: what they are, then the list and the notices,
/// folded (a few hundred lines unfolded); `platform`: whose build
/// ([`super::data::Platform::l10n`]).
fn rust(r: &RustCredits, platform: &str) -> Vec<Block> {
    let mut b = vec![
        Block::Heading(pm::credits_rust().into()),
        Block::Text(pm::credits_rust_about(platform, r.crates.len()).into()),
    ];
    b.push(Block::Fold {
        id: "rust-crates".into(),
        label: pm::credits_rust_crates(r.crates.len()).into(),
        body: r
            .crates
            .iter()
            .map(|k| {
                let notices = k
                    .notices
                    .iter()
                    .map(|i| (i + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                Block::Muted(
                    if k.chosen == k.license {
                        pm::credits_rust_crate(&k.name, &k.version, &k.license, notices)
                    } else {
                        pm::credits_rust_crate_chosen(
                            &k.name, &k.version, &k.license, &k.chosen, notices,
                        )
                    }
                    .into(),
                )
            })
            .collect(),
    });
    b.push(Block::Fold {
        id: "rust-notices".into(),
        label: pm::credits_rust_notices(r.notices.len()).into(),
        body: r
            .notices
            .iter()
            .enumerate()
            .map(|(i, n)| {
                // A fold's label is one line: a few names, the rest inside.
                let number = (i + 1).to_string();
                let label = match n.crates.as_slice() {
                    [a, b, rest @ ..] if !rest.is_empty() => {
                        pm::credits_rust_notice_more(&number, &n.license, a, b, rest.len())
                    }
                    names => pm::credits_rust_notice(&number, &n.license, names.join(", ")),
                };
                let mut body = Vec::new();
                if n.crates.len() > 2 {
                    body.push(Block::Muted(
                        pm::credits_rust_notice_for(n.crates.join(", ")).into(),
                    ));
                }
                body.push(Block::Mono(n.text.clone()));
                Block::Fold {
                    id: format!("rust-notice{i}"),
                    label: label.into(),
                    body,
                }
            })
            .collect(),
    });
    b
}

/// While the file loads, or when it could not be read.
pub fn waiting(error: Option<&str>) -> PageDoc {
    PageDoc {
        title: title(),
        blocks: vec![match error {
            None => Block::Muted(pm::credits_loading().into()),
            Some(e) => Block::Text(pm::credits_unreadable(super::errors::error_text(e)).into()),
        }],
    }
}

/// A small sample (the developer routes, tests): `sample::CREDITS`.
pub fn sample() -> Credits {
    parse(super::sample::CREDITS).expect("the sample parses")
}

#[cfg(test)]
mod rust_tests {
    use super::*;

    /// The Rust libraries come last, folded: the list and the notices.
    #[test]
    fn rust_libraries_are_a_folded_section() {
        let d = doc(&sample(), Default::default());
        let at = d
            .blocks
            .iter()
            .position(|b| *b == Block::Heading(pm::credits_rust().into()))
            .expect("the section");
        let folds: Vec<&str> = d.blocks[at..]
            .iter()
            .filter_map(|b| match b {
                Block::Fold { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(folds, ["rust-crates", "rust-notices"]);
        let Some(Block::Fold { body, .. }) = d
            .blocks
            .iter()
            .find(|b| matches!(b, Block::Fold { id, .. } if id == "rust-crates"))
        else {
            unreachable!()
        };
        assert_eq!(
            body[1],
            Block::Muted("serde 1.0.0 — MIT OR Apache-2.0, used under MIT (notice 2)".into())
        );
    }

    /// Our announcers follow the noteskins, each voice's recordings
    /// credited with a link to them.
    #[test]
    fn announcers_credit_what_their_voices_learned_from() {
        let d = doc(&sample(), Default::default());
        let at = |b: &Block| d.blocks.iter().position(|x| x == b).expect("the block");
        let heading = at(&Block::Heading(pm::credits_announcers().into()));
        assert!(heading > at(&Block::Heading(pm::credits_noteskins().into())));
        assert!(heading < at(&Block::Heading(pm::credits_fonts().into())));
        assert_eq!(
            d.blocks[heading + 1],
            Block::Subheading("ガキンさん — すごいジャン".into())
        );
        assert!(
            d.blocks.contains(&Block::Link {
                label: "its voice (jm_kumo) learned from A reading (a corpus): A broadcaster — \
                    CC-BY-3.0"
                    .into(),
                url: "https://example.org/reading".into(),
            })
        );
    }

    /// The generated files parse, when `xtask gen-credits` has run.
    #[test]
    fn generated_credits_parse() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/credits");
        for f in ["credits.json", "credits-desktop.json"] {
            let Ok(text) = std::fs::read_to_string(dir.join(f)) else {
                continue;
            };
            let c = parse(&text).unwrap_or_else(|e| panic!("{f}: {e}"));
            let r = c.rust.expect("its Rust libraries");
            assert!(!r.crates.is_empty() && !r.notices.is_empty());
            assert!(
                r.crates
                    .iter()
                    .flat_map(|k| &k.notices)
                    .all(|&i| i < r.notices.len())
            );
        }
    }
}
