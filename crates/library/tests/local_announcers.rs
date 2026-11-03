//! `DDI_ANNOUNCERS=<dir>[:<dir>…] cargo test -p ddi-library --test
//! local_announcers -- --ignored --nocapture` finds the announcers in each
//! folder and zip (`.zip`, `.smzip`) under local folders, as an import
//! would, and prints what each holds and passed over (third-party packs
//! stay local; `docs/plans/announcers.md`, step 1).

use std::path::{Path, PathBuf};

use ddi_library::announcer::{self, Found, Spoken};
use ddi_library::{pack, zip};

/// The files under `dir` (`(path relative to it, bytes)`).
fn listing(dir: &Path, base: &Path, out: &mut Vec<(String, u64)>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            listing(&path, base, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            out.push((rel, entry.metadata().map(|m| m.len()).unwrap_or(0)));
        }
    }
}

/// A zip's entries (`(name, bytes)`), names in no declared encoding
/// decoded as the shells decode them (`pack::decode_legacy_names`: UTF-8
/// when they all read as it, as macOS's zips do; CP437 here otherwise,
/// where the shells try the legacy code pages first).
fn zip_listing(path: &Path) -> Result<Vec<(String, u64)>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let len = bytes.len() as u64;
    let tail = &bytes[(len - zip::tail_len(len)) as usize..];
    let cd = match zip::locate_central_directory(tail, len).map_err(|e| e.to_string())? {
        zip::Locate::Found(cd) => cd,
        zip::Locate::NeedZip64Record { offset, len } => {
            zip::parse_zip64_end_record(&bytes[offset as usize..(offset + len) as usize])
                .map_err(|e| e.to_string())?
        }
    };
    let at = cd.offset as usize;
    let mut entries = zip::parse_central_directory(&bytes[at..at + cd.size as usize], &cd)
        .map_err(|e| e.to_string())?;
    let raws: Vec<&[u8]> = entries
        .iter()
        .filter_map(|e| e.legacy_name.as_deref())
        .collect();
    let mut names = pack::decode_legacy_names(&raws, |_, _| None).into_iter();
    for e in entries.iter_mut().filter(|e| e.legacy_name.is_some()) {
        e.name = names.next().unwrap_or_default();
    }
    Ok(entries
        .into_iter()
        .filter(|e| !e.is_dir)
        .map(|e| (e.name, e.uncompressed_size))
        .collect())
}

fn print(source: &Path, found: &Found) {
    println!("{}", source.display());
    if found.announcers.is_empty() {
        println!("  no announcer lines found");
    }
    for a in &found.announcers {
        let by = |s: Spoken| a.lines.keys().filter(|l| l.spoken() == s).count();
        println!(
            "  {:?} at {:?}: {} lines (play {}, results {}, menus {}), {} unused, {} unknown; {} sounds, {:.1} MB",
            a.name,
            a.dir.as_str(),
            a.lines.len(),
            by(Spoken::Play),
            by(Spoken::Results),
            by(Spoken::Menu),
            a.unused.len(),
            a.unknown.len(),
            a.report.files,
            a.report.bytes as f64 / 1e6,
        );
        let most = a.lines.iter().max_by_key(|(_, f)| f.len());
        if let Some((line, files)) = most {
            println!("    most variants: {} ({})", line.folder(), files.len());
        }
        if !a.unknown.is_empty() {
            let names: Vec<&str> = a.unknown.keys().map(String::as_str).collect();
            println!("    unknown: {}", names.join(", "));
        }
        let r = &a.report;
        if r.ignored + r.left_out > 0 || !r.empty.is_empty() || !r.nested.is_empty() {
            println!(
                "    ignored {} files, left out {}, empty {:?}, nested {:?}",
                r.ignored,
                r.left_out,
                r.empty,
                r.nested.iter().map(|n| n.as_str()).collect::<Vec<_>>()
            );
        }
    }
    if found.left_out > 0 {
        println!("  {} more announcers left out", found.left_out);
    }
}

#[test]
#[ignore = "needs DDI_ANNOUNCERS"]
fn local_announcers() {
    let Ok(dirs) = std::env::var("DDI_ANNOUNCERS") else {
        eprintln!("DDI_ANNOUNCERS is not set");
        return;
    };
    let mut sources: Vec<PathBuf> = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        sources.push(dir.clone());
        let mut zips = Vec::new();
        listing(&dir, &dir, &mut zips);
        sources.extend(zips.into_iter().map(|(p, _)| dir.join(p)).filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("smzip"))
        }));
    }
    for source in sources {
        let root = source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let files = if source.is_dir() {
            let mut files = Vec::new();
            listing(&source, &source, &mut files);
            files
        } else {
            match zip_listing(&source) {
                Ok(files) => files,
                Err(e) => {
                    println!("{}: {e}", source.display());
                    continue;
                }
            }
        };
        print(&source, &announcer::find_announcers(&files, &root));
    }
}
