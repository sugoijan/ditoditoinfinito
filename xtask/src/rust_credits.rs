//! The Rust libraries the game ships, with the notices their licences ask
//! for (part of `gen-credits`): the crates compiled into the web build
//! (the app for `wasm32-unknown-unknown` and the Opus worker) and into the
//! desktop binary (the app for Linux, macOS and Windows), each with its
//! licence expression, the licence we take it under where it offers a
//! choice, and that licence's text from the crate's own files (the SPDX
//! text from `LICENSES/` when it ships none), the texts deduplicated.
//!
//! The crates come from `cargo tree` (normal dependencies, no proc macros:
//! their code is not in the binaries; features resolved as each build
//! resolves them), their files from `cargo metadata`. A licence outside
//! [`ALLOWED`] fails the generator, so a copyleft crate cannot slip in.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The licences a shipped crate may be used under, most preferred first
/// (where a crate offers a choice, the first here is taken). `WITH
/// LLVM-exception` is allowed on any of them.
const ALLOWED: &[&str] = &[
    "MIT",
    "MIT-0",
    "Apache-2.0",
    "BSD-3-Clause",
    "BSD-2-Clause",
    "ISC",
    "Zlib",
    "0BSD",
    "Unlicense",
    "CC0-1.0",
    "BSL-1.0",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "MPL-2.0",
];
const EXCEPTIONS: &[&str] = &["LLVM-exception"];

/// The desktop's targets (what CI builds); the list is their union.
const DESKTOP_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RustCredits {
    /// The web build's crates (the app and the Opus worker).
    pub web: Vec<Crate>,
    /// The desktop binary's.
    pub desktop: Vec<Crate>,
    /// The licence texts, each once; crates point at them by index.
    pub notices: Vec<Notice>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Crate {
    pub name: String,
    pub version: String,
    /// The crate's licence expression, as it declares it.
    pub license: String,
    /// What we take it under (one side of an `OR`).
    pub chosen: String,
    pub repository: String,
    /// Indices into [`RustCredits::notices`].
    pub notices: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Notice {
    /// The SPDX id of the licence (or `NOTICE` for an Apache NOTICE file).
    pub license: String,
    pub text: String,
}

/// One build's list as a credits file carries it: its crates and only
/// the notices they use, renumbered.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Section {
    pub crates: Vec<Crate>,
    pub notices: Vec<SectionNotice>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SectionNotice {
    pub license: String,
    pub text: String,
    /// `name version` of the crates it covers.
    pub crates: Vec<String>,
}

impl RustCredits {
    pub fn section(&self, crates: &[Crate]) -> Section {
        let mut map: BTreeMap<usize, usize> = BTreeMap::new();
        let mut notices: Vec<SectionNotice> = Vec::new();
        let crates = crates
            .iter()
            .map(|c| {
                let mut c = c.clone();
                for i in &mut c.notices {
                    let n = *map.entry(*i).or_insert_with(|| {
                        notices.push(SectionNotice {
                            license: self.notices[*i].license.clone(),
                            text: self.notices[*i].text.clone(),
                            crates: Vec::new(),
                        });
                        notices.len() - 1
                    });
                    notices[n].crates.push(format!("{} {}", c.name, c.version));
                    *i = n;
                }
                c
            })
            .collect();
        Section { crates, notices }
    }
}

/// Collects the lists, reusing `target/credits/rust.json` while
/// `Cargo.lock` and the workspace's manifests are unchanged (Trunk runs
/// the generator before every build).
pub fn collect(root: &Path) -> Result<RustCredits> {
    let key = cache_key(root)?;
    let cache = root.join("target/credits/rust.json");
    let key_file = root.join("target/credits/rust.key");
    if fs::read_to_string(&key_file).is_ok_and(|k| k == key)
        && let Ok(text) = fs::read_to_string(&cache)
        && let Ok(c) = serde_json::from_str::<RustCredits>(&text)
    {
        return Ok(c);
    }
    let c = generate(root)?;
    fs::create_dir_all(root.join("target/credits"))?;
    fs::write(&cache, serde_json::to_string(&c)?)?;
    fs::write(&key_file, key)?;
    Ok(c)
}

/// What the lists depend on: the lock file, every manifest in the
/// workspace and this generator.
fn cache_key(root: &Path) -> Result<String> {
    let mut parts = vec![fs::read_to_string(root.join("Cargo.lock"))?];
    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    let workspace: toml::Value = toml::from_str(&manifest)?;
    let members = workspace["workspace"]["members"]
        .as_array()
        .context("workspace members must be explicit paths")?;
    for member in members {
        let member = member.as_str().context("workspace member must be a path")?;
        let text = fs::read_to_string(root.join(member).join("Cargo.toml"))?;
        parts.push(format!("{member}\n{text}"));
    }
    parts.push(manifest);
    parts.sort();
    parts.push(include_str!("rust_credits.rs").to_string());
    // FNV-1a over all of it: a change anywhere changes the key.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in parts.concat().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    Ok(format!("{h:016x}"))
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

/// `(name, version)` of every crates.io (or git) crate `package` builds
/// with for `targets`.
fn tree(root: &Path, package: &str, targets: &[&str]) -> Result<BTreeSet<(String, String)>> {
    tree_features(root, package, targets, &[])
}

fn tree_features(
    root: &Path,
    package: &str,
    targets: &[&str],
    features: &[&str],
) -> Result<BTreeSet<(String, String)>> {
    let mut cmd = Command::new(cargo());
    cmd.current_dir(root).args([
        "tree",
        "--quiet",
        "--edges",
        "normal,no-proc-macro",
        "--prefix",
        "none",
        "--format",
        "{p}",
        "--package",
        package,
    ]);
    for t in targets {
        cmd.args(["--target", t]);
    }
    if !features.is_empty() {
        cmd.args(["--features", &features.join(",")]);
    }
    let out = cmd.output().context("running cargo tree")?;
    if !out.status.success() {
        bail!(
            "cargo tree -p {package}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let mut set = BTreeSet::new();
    for line in String::from_utf8(out.stdout)?.lines() {
        let mut words = line.split_whitespace();
        let (Some(name), Some(version)) = (words.next(), words.next()) else {
            continue;
        };
        let rest: Vec<&str> = words.collect();
        // Path crates (ours) print their folder.
        if rest
            .first()
            .is_some_and(|w| w.starts_with("(/") || w.contains(":\\"))
        {
            continue;
        }
        set.insert((
            name.to_string(),
            version.trim_start_matches('v').to_string(),
        ));
    }
    Ok(set)
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    license: Option<String>,
    license_file: Option<String>,
    manifest_path: PathBuf,
    #[serde(default)]
    authors: Vec<String>,
    repository: Option<String>,
    source: Option<String>,
}

fn generate(root: &Path) -> Result<RustCredits> {
    let mut web = tree(root, "ditoditoinfinito", &["wasm32-unknown-unknown"])?;
    web.extend(tree(root, "ddi-opus-worker", &["wasm32-unknown-unknown"])?);
    let mut desktop = tree(root, "ditoditoinfinito", DESKTOP_TARGETS)?;
    // The video decoder helper ships beside the game (`video/helper`).
    desktop.extend(tree(root, "ddi-video-helper", DESKTOP_TARGETS)?);
    // The optional native importer adds only its pure protocol, URL parser and
    // integrity hash to this binary. The helper/worker runtime dependency graphs
    // ship their own complete notices inside the content-addressed bundle.
    desktop.extend(tree_features(
        root,
        "ditoditoinfinito",
        &["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"],
        &["danoni-importer"],
    )?);
    let out = Command::new(cargo())
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--all-features"])
        .output()
        .context("running cargo metadata")?;
    if !out.status.success() {
        bail!("cargo metadata: {}", String::from_utf8_lossy(&out.stderr));
    }
    let meta: Metadata = serde_json::from_slice(&out.stdout).context("reading cargo metadata")?;
    let packages: BTreeMap<(String, String), &Package> = meta
        .packages
        .iter()
        .filter(|p| p.source.is_some())
        .map(|p| ((p.name.clone(), p.version.clone()), p))
        .collect();
    let mut notices: Vec<Notice> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut crates: BTreeMap<(String, String), Crate> = BTreeMap::new();
    let mut problems = Vec::new();
    for key in web.iter().chain(&desktop) {
        if crates.contains_key(key) {
            continue;
        }
        let Some(p) = packages.get(key) else {
            problems.push(format!("{} {}: not in cargo metadata", key.0, key.1));
            continue;
        };
        match credit(root, p, &mut notices, &mut seen) {
            Ok(c) => {
                crates.insert(key.clone(), c);
            }
            Err(e) => problems.push(format!("{} {}: {e}", p.name, p.version)),
        }
    }
    if !problems.is_empty() {
        bail!(
            "Rust dependencies the credits cannot cover:\n  {}",
            problems.join("\n  ")
        );
    }
    let list = |set: &BTreeSet<(String, String)>| set.iter().map(|k| crates[k].clone()).collect();
    Ok(RustCredits {
        web: list(&web),
        desktop: list(&desktop),
        notices,
    })
}

/// A crate's entry, its notices added to `notices` (deduplicated by their
/// text, whitespace aside).
fn credit(
    root: &Path,
    p: &Package,
    notices: &mut Vec<Notice>,
    seen: &mut BTreeMap<String, usize>,
) -> Result<Crate> {
    let expr = p
        .license
        .clone()
        .context("no licence expression in its manifest")?;
    let dir = p
        .manifest_path
        .parent()
        .context("a manifest without a folder")?;
    let files = licence_files(dir, p.license_file.as_deref());
    // The most preferred side whose texts the crate ships; else one whose
    // texts need no holder's name; else the most preferred.
    let options = choices(&expr)?;
    let base = |id: &String| id.split(" WITH ").next().unwrap_or(id).to_string();
    let shipped = |alt: &Vec<String>| {
        alt.iter()
            .all(|id| files.iter().any(|(_, t)| contains(t, &base(id))))
    };
    let holderless =
        |alt: &Vec<String>| alt.iter().all(|id| HOLDERLESS.contains(&base(id).as_str()));
    let chosen = options
        .iter()
        .find(|a| shipped(a))
        .or_else(|| options.iter().find(|a| holderless(a)))
        .or(options.first())
        .cloned()
        .with_context(|| format!("licence {expr:?} is not on the allowed list"))?;
    let mut texts = Vec::new();
    for id in &chosen {
        let base = id.split(" WITH ").next().unwrap_or(id);
        // A file holding the licence's text ships whole (with whatever
        // else it says), the one its name points at first.
        let mut text = match files
            .iter()
            .find(|(name, text)| contains(text, base) && hints(name, base))
        {
            Some((_, t)) => t.clone(),
            None => match files.iter().find(|(_, text)| contains(text, base)) {
                Some((_, t)) => t.clone(),
                None => fallback(root, base, p)?,
            },
        };
        // The exception's text with the licence's, where the file lacks it.
        if id.ends_with(" WITH LLVM-exception")
            && !text.contains("LLVM Exceptions to the Apache 2.0 License")
        {
            text = format!("{}\n\n{LLVM_EXCEPTION}", text.trim_end());
        }
        texts.push((id.to_string(), text));
        if base == "Apache-2.0" {
            for (name, text) in &files {
                if name.to_ascii_uppercase().starts_with("NOTICE") {
                    texts.push(("NOTICE".into(), text.clone()));
                }
            }
        }
    }
    let mut idx = Vec::new();
    for (license, text) in texts {
        let text = text.replace("\r\n", "\n").trim().to_string();
        let key: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let i = *seen.entry(key).or_insert_with(|| {
            notices.push(Notice { license, text });
            notices.len() - 1
        });
        if !idx.contains(&i) {
            idx.push(i);
        }
    }
    Ok(Crate {
        name: p.name.clone(),
        version: p.version.clone(),
        license: expr,
        chosen: chosen.join(" AND "),
        repository: p.repository.clone().unwrap_or_default(),
        notices: idx,
    })
}

/// The crate's licence files (top level): `LICENSE*`, `LICENCE*`,
/// `COPYING*`, `UNLICENSE*`, `NOTICE*`, `*-LICENSE*` and the manifest's
/// `license-file`, with their text.
fn licence_files(dir: &Path, license_file: Option<&str>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    for name in names {
        let up = name.to_ascii_uppercase();
        let wanted = ["LICENSE", "LICENCE", "COPYING", "UNLICENSE", "NOTICE"]
            .iter()
            .any(|p| up.starts_with(p))
            || up.contains("-LICENSE")
            || up.contains("_LICENSE");
        let licence_file =
            license_file.is_some_and(|f| Path::new(f).file_name() == Some(name.as_ref()));
        if (wanted || licence_file)
            && let Ok(text) = fs::read_to_string(dir.join(&name))
        {
            out.push((name, text));
        }
    }
    out
}

/// Whether `text` holds licence `id`'s wording (a file may hold several:
/// libm's has MIT's, Apache's and its sources' own notices). Loose where
/// crates word a licence their own way (bytemuck's MIT says "(including
/// the next paragraph)").
fn contains(text: &str, id: &str) -> bool {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let has = |s: &str| t.contains(s);
    let mit_grant = has(
        "Permission is hereby granted, free of charge, to any person obtaining a copy of this software",
    );
    match id {
        "MIT" => mit_grant && has("copyright notice and this permission notice"),
        "MIT-0" => mit_grant,
        "Apache-2.0" => has("Apache License") && has("Version 2.0") && has("TERMS AND CONDITIONS"),
        "BSD-3-Clause" => {
            has("Redistribution and use in source and binary forms") && has("endorse or promote")
        }
        "BSD-2-Clause" => has("Redistribution and use in source and binary forms"),
        "ISC" => {
            has("Permission to use, copy, modify, and/or distribute this software")
                && has("provided that the above copyright notice")
        }
        "0BSD" => has("Permission to use, copy, modify, and/or distribute this software"),
        "Zlib" => has("provided 'as-is', without any express or implied"),
        "MPL-2.0" => has("Mozilla Public License Version 2.0"),
        "Unlicense" => {
            has("This is free and unencumbered software released into the public domain")
        }
        "CC0-1.0" => has("CC0 1.0 Universal"),
        "BSL-1.0" => has("Boost Software License - Version 1.0"),
        "Unicode-3.0" => has("UNICODE LICENSE V3"),
        "Unicode-DFS-2016" => has("UNICODE, INC. LICENSE AGREEMENT"),
        _ => false,
    }
}

/// Whether a file's name points at `id` (a crate offering several
/// licences ships a file each), or names none.
fn hints(name: &str, id: &str) -> bool {
    let up = name.to_ascii_uppercase();
    let marks: &[&str] = match id {
        "MIT" | "MIT-0" => &["MIT"],
        "Apache-2.0" => &["APACHE"],
        "BSD-2-Clause" | "BSD-3-Clause" => &["BSD"],
        "ISC" => &["ISC"],
        "Zlib" => &["ZLIB"],
        "Unlicense" => &["UNLICENSE"],
        "BSL-1.0" => &["BSL", "BOOST"],
        "MPL-2.0" => &["MPL"],
        "Unicode-3.0" | "Unicode-DFS-2016" => &["UNICODE"],
        "CC0-1.0" => &["CC0"],
        _ => &[],
    };
    marks.iter().any(|m| up.contains(m))
}

/// The Boost licence's text (it names no holder, so the standard text is
/// the notice): clipboard-win, in the Windows build, ships none, and the
/// repository has no BSL-1.0 file of its own for `LICENSES/`.
const BSL_1_0: &str = "Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the \"Software\") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third-parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the above license grant, this restriction and the following disclaimer,
must be included in all copies of the Software, in whole or in part, and
all derivative works of the Software, unless such copies or derivative
works are solely in the form of machine-executable object code generated by
a source language processor.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT
SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE
FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
";

/// The LLVM exception's text (SPDX `LLVM-exception`), added to an
/// Apache-2.0 text that lacks it.
const LLVM_EXCEPTION: &str = "--- LLVM Exceptions to the Apache 2.0 License ----

As an exception, if, as a result of your compiling your source code, portions
of this Software are embedded into an Object form of such source code, you
may redistribute such embedded portions in such Object form without complying
with the conditions of Sections 4(a), 4(b) and 4(d) of the License.

In addition, if you combine or link compiled forms of this Software with
software that is licensed under the GPLv2 (\"Combined Software\") and if a
court of competent jurisdiction determines that the patent provision (Section
3), the indemnity provision (Section 9) or other Section of the License
conflicts with the conditions of the GPLv2, you may retroactively and
prospectively choose to deem waived or otherwise exclude such Section(s) of
the License, but only in their entirety and only with respect to the Combined
Software.
";

/// The licence's text when a crate ships none: the SPDX text from
/// `LICENSES/` (MIT with the crate's authors as the copyright line), or
/// one kept here for a licence the repository does not use itself.
fn fallback(root: &Path, id: &str, p: &Package) -> Result<String> {
    if id == "BSL-1.0" {
        return Ok(BSL_1_0.to_string());
    }
    let path = root.join("LICENSES").join(format!("{id}.txt"));
    let text = fs::read_to_string(&path)
        .with_context(|| format!("ships no {id} text, and LICENSES/{id}.txt is missing"))?;
    if id == "MIT" {
        let holders = if p.authors.is_empty() {
            format!("the {} authors", p.name)
        } else {
            p.authors.join(", ")
        };
        return Ok(text.replace(
            "Copyright (c) <year> <copyright holders>",
            &format!("Copyright (c) {holders}"),
        ));
    }
    Ok(text)
}

/// The licences to take a crate under: the most preferred side of its
/// expression whose licences are all allowed.
#[cfg(test)]
fn choose(expr: &str) -> Result<Vec<String>> {
    choices(expr)?
        .into_iter()
        .next()
        .with_context(|| format!("licence {expr:?} is not on the allowed list"))
}

/// Every side of `expr` whose licences are all allowed, most preferred
/// first.
fn choices(expr: &str) -> Result<Vec<Vec<String>>> {
    let dnf = parse(expr).with_context(|| format!("cannot read the licence {expr:?}"))?;
    let rank = |id: &str| {
        let base = id.split(" WITH ").next().unwrap_or(id);
        let exception_ok = id
            .split(" WITH ")
            .nth(1)
            .is_none_or(|e| EXCEPTIONS.contains(&e));
        ALLOWED
            .iter()
            .position(|a| *a == base)
            .filter(|_| exception_ok)
    };
    let mut ranked: Vec<(usize, Vec<String>)> = dnf
        .into_iter()
        .filter_map(|alt| {
            let ranks: Option<Vec<usize>> = alt.iter().map(|id| rank(id)).collect();
            ranks.map(|r| (r.iter().sum::<usize>() + 100 * (alt.len() - 1), alt))
        })
        .collect();
    ranked.sort_by_key(|(r, _)| *r);
    Ok(ranked.into_iter().map(|(_, alt)| alt).collect())
}

/// Licences whose standard text names no holder: a crate that ships no
/// licence file is taken under one of these when it offers one, rather
/// than under a licence whose notice would need a copyright line it does
/// not give.
const HOLDERLESS: &[&str] = &["Apache-2.0", "BSL-1.0", "MPL-2.0", "CC0-1.0", "Unlicense"];

/// An SPDX expression (and the old `A/B` form) in disjunctive normal form:
/// alternatives, each the licences it needs together.
fn parse(expr: &str) -> Result<Vec<Vec<String>>> {
    let spaced = expr
        .replace('(', " ( ")
        .replace(')', " ) ")
        .replace('/', " OR ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut at = 0;
    let dnf = or(&tokens, &mut at)?;
    if at != tokens.len() {
        bail!("unexpected {:?}", tokens[at]);
    }
    Ok(dnf)
}

fn or(t: &[&str], at: &mut usize) -> Result<Vec<Vec<String>>> {
    let mut out = and(t, at)?;
    while t.get(*at).is_some_and(|w| w.eq_ignore_ascii_case("OR")) {
        *at += 1;
        out.extend(and(t, at)?);
    }
    Ok(out)
}

fn and(t: &[&str], at: &mut usize) -> Result<Vec<Vec<String>>> {
    let mut out = atom(t, at)?;
    while t.get(*at).is_some_and(|w| w.eq_ignore_ascii_case("AND")) {
        *at += 1;
        let right = atom(t, at)?;
        let mut next = Vec::new();
        for a in &out {
            for b in &right {
                let mut c = a.clone();
                c.extend(b.iter().cloned());
                next.push(c);
            }
        }
        out = next;
    }
    Ok(out)
}

fn atom(t: &[&str], at: &mut usize) -> Result<Vec<Vec<String>>> {
    match t.get(*at) {
        Some(&"(") => {
            *at += 1;
            let inner = or(t, at)?;
            if t.get(*at) != Some(&")") {
                bail!("a parenthesis is not closed");
            }
            *at += 1;
            Ok(inner)
        }
        Some(id) => {
            *at += 1;
            let mut id = id.trim_end_matches('+').to_string();
            if t.get(*at).is_some_and(|w| w.eq_ignore_ascii_case("WITH")) {
                let e = t.get(*at + 1).context("WITH what?")?;
                id = format!("{id} WITH {e}");
                *at += 2;
            }
            Ok(vec![vec![id]])
        }
        None => bail!("the expression ends early"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_are_read_and_chosen() {
        assert_eq!(choose("MIT OR Apache-2.0").unwrap(), ["MIT"]);
        assert_eq!(choose("Apache-2.0/MIT").unwrap(), ["MIT"]);
        assert_eq!(
            choose("Apache-2.0 OR GPL-2.0-only").unwrap(),
            ["Apache-2.0"]
        );
        assert_eq!(
            choose("Apache-2.0 WITH LLVM-exception").unwrap(),
            ["Apache-2.0 WITH LLVM-exception"]
        );
        assert_eq!(
            choose("(MIT OR Apache-2.0) AND Unicode-3.0").unwrap(),
            ["MIT", "Unicode-3.0"]
        );
        assert_eq!(
            choose("(Apache-2.0 OR MIT) AND BSD-3-Clause").unwrap(),
            ["MIT", "BSD-3-Clause"]
        );
        assert_eq!(choose("0BSD OR MIT OR Apache-2.0").unwrap(), ["MIT"]);
        assert!(choose("GPL-3.0-only").is_err());
        assert!(choose("MIT AND GPL-2.0-or-later").is_err());
        assert!(choose("LGPL-2.1-or-later OR GPL-2.0").is_err());
        assert!(choose("Apache-2.0 WITH Classpath-exception-2.0").is_err());
        assert!(choose("(MIT").is_err());
    }

    #[test]
    fn texts_are_told_apart() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let read = |f: &str| fs::read_to_string(root.join("LICENSES").join(f)).unwrap();
        assert!(contains(&read("MIT.txt"), "MIT"));
        assert!(!contains(&read("MIT.txt"), "Apache-2.0"));
        assert!(contains(&read("Apache-2.0.txt"), "Apache-2.0"));
        assert!(contains(&read("Unlicense.txt"), "Unlicense"));
        assert!(!contains("nothing", "MIT"));
        // bytemuck's wording, and a file holding two licences (libm's).
        let bytemuck = "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software... The above copyright notice and this permission notice (including the next paragraph) shall be included in all copies";
        assert!(contains(bytemuck, "MIT"));
        let both = format!("{}\n{}", read("MIT.txt"), read("Apache-2.0.txt"));
        assert!(contains(&both, "MIT") && contains(&both, "Apache-2.0"));
        // The Boost and Unicode grants are not MIT's.
        assert!(!contains(BSL_1_0, "MIT") && contains(BSL_1_0, "BSL-1.0"));
        assert!(hints("LICENSE-MIT", "MIT") && !hints("LICENSE-APACHE", "MIT"));
    }
}
