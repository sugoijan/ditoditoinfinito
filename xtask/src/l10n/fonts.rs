//! The languages' faces (`locales/languages.toml`'s `[[face]]`s,
//! `docs/plans/localization.md`, "Text and fonts").
//!
//! A language whose script the theme's text face (the Iosevka Term
//! subset) lacks names faces of its own; each is cut from its pinned
//! upstream file to its `base` plus every character its languages'
//! messages and names write that the theme's face does not draw. [`cut`]
//! makes the cuts (`assets/fonts/`, committed; the command is in
//! `assets/fonts/PROVENANCE.toml`); [`check`], part of `l10n check`,
//! fails a message writing a character none of its language's faces
//! draws, and a face's file missing, uncredited or not served.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::Digest;

use super::{LanguageEntry, catalog, listed};

/// The theme's text face (`crates/theme/src/infinito.toml`'s `fonts.text`,
/// first in every stack): what it draws, no language's face needs to.
const THEME_FACE: &str = "assets/fonts/IosevkaTerm-Regular-latin.ttf";

/// Drawn by the system's fonts on purpose (`crates/theme/tests/glyphs.rs`
/// says why): never asked of a face.
const SYSTEM: &[char] = &['☆', '★'];

/// The fonttools release the cuts are made with.
const FONTTOOLS: &str = "fonttools==4.68.0";

/// The code points `path`'s character map has.
fn coverage(path: &Path) -> Result<HashSet<u32>> {
    let data = fs::read(path).with_context(|| path.display().to_string())?;
    let face = ttf_parser::Face::parse(&data, 0)
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    let mut set = HashSet::new();
    if let Some(cmap) = face.tables().cmap {
        for table in cmap.subtables {
            if table.is_unicode() {
                table.codepoints(|c| {
                    set.insert(c);
                });
            }
        }
    }
    Ok(set)
}

/// Whether a face is never asked for `c`.
fn exempt(c: char) -> bool {
    c.is_whitespace() || c.is_control() || SYSTEM.contains(&c)
}

/// The characters the languages using `family` write that the theme's
/// face lacks: their messages' and terms' text and their names.
fn needed(
    root: &Path,
    users: &[&LanguageEntry],
    theme: &HashSet<u32>,
) -> Result<BTreeMap<char, String>> {
    let mut chars = BTreeMap::new();
    for l in users {
        let c = catalog(root, &l.tag)?;
        for (ch, id) in c.chars {
            chars.entry(ch).or_insert(format!("{}: {id}", l.tag));
        }
        for ch in l.name.chars() {
            chars.entry(ch).or_insert(format!("{}: its name", l.tag));
        }
    }
    chars.retain(|c, _| !exempt(*c) && !theme.contains(&u32::from(*c)));
    Ok(chars)
}

/// Every weight of every face, cut.
pub(super) fn cut(root: &Path) -> Result<()> {
    let listed = listed(root)?;
    let theme = coverage(&root.join(THEME_FACE))?;
    let cache = root.join("target/fonts");
    fs::create_dir_all(cache.join("src"))?;
    for face in &listed.face {
        let users: Vec<&LanguageEntry> = listed
            .language
            .iter()
            .filter(|l| l.faces.contains(&face.family))
            .collect();
        let chars = needed(root, &users, &theme)?;
        let text: String = chars.keys().collect();
        let list = cache.join(format!("{}.txt", face.family.replace(' ', "")));
        fs::write(&list, &text)?;
        println!(
            "{}: {} characters beyond its base ({})",
            face.family,
            chars.len(),
            users
                .iter()
                .map(|l| l.tag.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        for f in &face.file {
            let source = fetch(&cache.join("src"), &f.source, &f.sha256)?;
            let out = root.join("assets/fonts").join(&f.file);
            let status = Command::new("uv")
                .args(["run", "--no-project", "--with", FONTTOOLS, "pyftsubset"])
                .arg(&source)
                .arg(format!("--unicodes={}", face.base))
                .arg(format!("--text-file={}", list.display()))
                .arg(format!("--layout-features={}", face.features))
                .arg("--name-IDs=*")
                .arg(format!("--output-file={}", out.display()))
                .status()
                .context("running pyftsubset through uv (https://docs.astral.sh/uv/)")?;
            if !status.success() {
                bail!("pyftsubset failed on {}", source.display());
            }
            println!(
                "  {} ({}): {} bytes",
                f.file,
                f.weight,
                fs::metadata(&out)?.len()
            );
        }
    }
    Ok(())
}

/// `url`'s file in `dir`, downloaded once and checked against `sha256`.
fn fetch(dir: &Path, url: &str, sha256: &str) -> Result<PathBuf> {
    let name = url.rsplit('/').next().unwrap_or("font");
    let path = dir.join(name);
    let ok = |p: &Path| -> Result<bool> {
        Ok(p.is_file() && hex(&sha2::Sha256::digest(fs::read(p)?)) == sha256)
    };
    if ok(&path)? {
        return Ok(path);
    }
    let part = dir.join(format!("{name}.part"));
    println!("  fetching {url}");
    let status = Command::new("curl")
        .args(["-sSfL", "-o"])
        .arg(&part)
        .arg(url)
        .status()
        .context("running curl")?;
    if !status.success() {
        bail!("could not download {url}");
    }
    if !ok(&part)? {
        bail!("{url} is not the file pinned (SHA-256 {sha256})");
    }
    fs::rename(&part, &path)?;
    Ok(path)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What is wrong with the faces: a message (or a language's name) writing
/// a character neither the theme's face nor its language's faces draw
/// (every weight), a face's file missing, not credited or not served.
pub(super) fn check(root: &Path) -> Result<Vec<String>> {
    let listed = listed(root)?;
    let theme = coverage(&root.join(THEME_FACE))?;
    let mut problems = Vec::new();
    let provenance = fs::read_to_string(root.join("assets/fonts/PROVENANCE.toml"))?;
    let index = fs::read_to_string(root.join("site/index.html"))?;
    let mut faces: BTreeMap<&str, Vec<(String, HashSet<u32>)>> = BTreeMap::new();
    for face in &listed.face {
        let mut files = Vec::new();
        for f in &face.file {
            let path = root.join("assets/fonts").join(&f.file);
            if !path.is_file() {
                problems.push(format!(
                    "assets/fonts/{} ({}) is missing: cargo run -p xtask -- l10n fonts",
                    f.file, face.family
                ));
                continue;
            }
            if !provenance.contains(&format!("path = \"{}\"", f.file)) {
                problems.push(format!(
                    "assets/fonts/{} has no [[file]] in assets/fonts/PROVENANCE.toml",
                    f.file
                ));
            }
            if !index.contains(&format!("href=\"../assets/fonts/{}\"", f.file)) {
                problems.push(format!(
                    "site/index.html does not copy assets/fonts/{} (a copy-file link \
                     beside the theme's faces, not preloaded)",
                    f.file
                ));
            }
            files.push((f.file.clone(), coverage(&path)?));
        }
        faces.insert(&face.family, files);
    }
    for l in &listed.language {
        let mut chars = catalog(root, &l.tag)?.chars;
        for ch in l.name.chars() {
            chars.entry(ch).or_insert("its name".into());
        }
        let mut missing: BTreeMap<char, (&str, BTreeSet<&str>)> = BTreeMap::new();
        for (ch, id) in &chars {
            let cp = u32::from(*ch);
            if exempt(*ch) || theme.contains(&cp) {
                continue;
            }
            let lacking: BTreeSet<&str> = l
                .faces
                .iter()
                .flat_map(|f| faces.get(f.as_str()).into_iter().flatten())
                .filter(|(_, set)| !set.contains(&cp))
                .map(|(file, _)| file.as_str())
                .collect();
            if l.faces.is_empty() || !lacking.is_empty() {
                missing.insert(*ch, (id.as_str(), lacking));
            }
        }
        for (ch, (id, lacking)) in missing {
            let tag = &l.tag;
            problems.push(if l.faces.is_empty() {
                format!(
                    "{tag}: {id} writes {ch} (U+{:04X}), which the theme's face lacks: \
                     add it to its subset (assets/fonts/PROVENANCE.toml) or give {tag} \
                     a face (locales/languages.toml)",
                    u32::from(ch)
                )
            } else {
                format!(
                    "{tag}: {id} writes {ch} (U+{:04X}), which {} {}: \
                     cargo run -p xtask -- l10n fonts",
                    u32::from(ch),
                    lacking.iter().copied().collect::<Vec<_>>().join(", "),
                    if lacking.len() == 1 { "lacks" } else { "lack" }
                )
            });
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_face_has_latin_and_not_kana() {
        let set = coverage(&super::super::root().join(THEME_FACE)).unwrap();
        assert!(set.contains(&u32::from('A')) && set.contains(&u32::from('ő')));
        assert!(!set.contains(&u32::from('あ')));
    }

    #[test]
    fn spaces_and_the_system_s_stars_are_exempt() {
        assert!(exempt(' ') && exempt('\n') && exempt('★'));
        assert!(!exempt('曲'));
    }
}
