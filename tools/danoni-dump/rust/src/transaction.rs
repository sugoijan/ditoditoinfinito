//! Journaled publication of validated generations. Native hosts may instead
//! consume prepared artifacts and implement their own library transaction.
use crate::{
    Error, Result,
    assets::{JSON_LIMIT, relative_path, sha256},
    native::{bounded_file, external, io},
    package::{document, root, save, verify_generation},
    work::{Progress, Stage},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
fn invalid(message: impl Into<String>) -> Error {
    Error::new("publication", message)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema: String,
    phase: String,
    stage: PathBuf,
    destination: PathBuf,
    archive: PathBuf,
    new_manifest_sha256: String,
    old_manifest_sha256: Option<String>,
}
fn sync_dir(path: &Path) -> Result<()> {
    fs::File::open(path).map_err(io)?.sync_all().map_err(io)
}
fn journal(path: &Path, j: &Journal) -> Result<()> {
    let temp = path.with_extension("tmp");
    // Only our known temporary journal file may be replaced after interruption.
    if temp.exists() {
        return Err(invalid("Temporary journal needs operator review"));
    }
    save(&temp, j)?;
    fs::rename(&temp, path).map_err(io)?;
    sync_dir(path.parent().unwrap())
}
fn hash_manifest(path: &Path) -> Result<String> {
    Ok(sha256(&bounded_file(
        &path.join("manifest.json"),
        JSON_LIMIT,
    )?))
}
fn location(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(io)?;
    external(&parent)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| invalid("Missing destination name"))?;
    relative_path(name)?;
    let p = parent.join(name);
    if fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(invalid("Publication symlink"));
    }
    Ok(p)
}
fn disjoint(paths: &[&Path]) -> Result<()> {
    for (i, a) in paths.iter().enumerate() {
        for b in &paths[i + 1..] {
            if a.starts_with(b) || b.starts_with(a) {
                return Err(invalid("Publication paths must be disjoint"));
            }
        }
    }
    Ok(())
}
fn lock(history: &Path) -> Result<fs::File> {
    let path = history.join(".publication.lock");
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(invalid("Lock symlink"));
    }
    let f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(io)?;
    f.try_lock()
        .map_err(|e| invalid(format!("Another publisher owns the history lock: {e}")))?;
    Ok(f)
}
fn destination_lock(destination: &Path) -> Result<fs::File> {
    let name = destination
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("Invalid destination"))?;
    let path = destination
        .parent()
        .unwrap()
        .join(format!(".{name}.publication.lock"));
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(invalid("Destination lock symlink"));
    }
    let f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(io)?;
    f.try_lock()
        .map_err(|e| invalid(format!("Another publisher owns destination: {e}")))?;
    Ok(f)
}

fn checked(
    path: &Path,
    expected: &str,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<()> {
    if hash_manifest(path)? != expected {
        return Err(invalid("Generation identity changed"));
    }
    verify_generation(path, control)?;
    Ok(())
}
fn recover_locked(path: &Path, control: &mut impl FnMut(Progress) -> Result<()>) -> Result<Value> {
    let mut j: Journal =
        serde_json::from_value(document(path)?).map_err(|e| invalid(e.to_string()))?;
    if j.schema != "ddi-danoni-publication/1"
        || ![
            "prepared",
            "old-archived",
            "promoted",
            "committed",
            "rolled-back",
        ]
        .contains(&j.phase.as_str())
    {
        return Err(invalid("Unsupported journal"));
    }
    let history = path.parent().unwrap();
    let stage = location(&j.stage)?;
    let destination = location(&j.destination)?;
    let archive = location(&j.archive)?;
    if stage != j.stage
        || destination != j.destination
        || archive != j.archive
        || archive.parent() != Some(history)
    {
        return Err(invalid("Journal locations changed"));
    }
    disjoint(&[&stage, &destination, history])?;
    let valid = |p: &Path, hash: &str, c: &mut _| -> Result<()> { checked(p, hash, c) };
    if destination.exists()
        && !stage.exists()
        && hash_manifest(&destination)? == j.new_manifest_sha256
    {
        valid(&destination, &j.new_manifest_sha256, control)?;
        match &j.old_manifest_sha256 {
            Some(h) => valid(&archive, h, control)?,
            None => {
                if archive.exists() {
                    return Err(invalid("Unexpected archive"));
                }
            }
        }
        j.phase = "committed".into();
    } else {
        valid(&stage, &j.new_manifest_sha256, control)?;
        match &j.old_manifest_sha256 {
            Some(old) => {
                if destination.exists() {
                    if archive.exists() {
                        return Err(invalid("Ambiguous old generation locations"));
                    }
                    valid(&destination, old, control)?;
                } else {
                    valid(&archive, old, control)?;
                    fs::rename(&archive, &destination).map_err(io)?;
                    sync_dir(history)?;
                    sync_dir(destination.parent().unwrap())?;
                }
            }
            None => {
                if destination.exists() || archive.exists() {
                    return Err(invalid("Unexpected publication locations"));
                }
            }
        }
        j.phase = "rolled-back".into();
    }
    let temp = path.with_extension("tmp");
    if fs::symlink_metadata(&temp).is_ok() {
        let m = fs::symlink_metadata(&temp).map_err(io)?;
        if !m.is_file() || m.file_type().is_symlink() {
            return Err(invalid("Unexpected temporary journal"));
        }
        let preserved = path.with_extension(format!(
            "interrupted-{}.tmp",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| invalid(e.to_string()))?
                .as_nanos()
        ));
        if preserved.exists() {
            return Err(invalid("Recovery receipt collision"));
        }
        fs::rename(&temp, &preserved).map_err(io)?;
        sync_dir(history)?;
    }
    journal(path, &j)?;
    Ok(
        json!({"schema":"ddi-danoni-publication-result/1","status":j.phase,"destination":destination,"stage":stage,"archive":if j.old_manifest_sha256.is_some() && archive.exists(){Some(archive)}else{None},"journal":path,"generationSha256":j.new_manifest_sha256}),
    )
}
/// Recover from observed directory identities, rather than trusting a journal's
/// phase (the crash may occur between a rename and its journal update).
pub fn recover(path: &Path, control: &mut impl FnMut(Progress) -> Result<()>) -> Result<Value> {
    let path = path.canonicalize().map_err(io)?;
    external(&path)?;
    let history = root(path.parent().unwrap())?;
    let _lock = lock(&history)?;
    let data: Journal =
        serde_json::from_value(document(&path)?).map_err(|e| invalid(e.to_string()))?;
    let destination = location(&data.destination)?;
    let _destination_lock = destination_lock(&destination)?;
    recover_locked(&path, control)
}
/// Archives an old *validated Rust generation* and atomically renames the new
/// one into place. No copying/deleting fallback on cross-filesystem failures.
/// Cancellation before the final rename restores the old generation. Once the
/// final rename succeeds, recovery reports committed, even if journaling failed.
pub fn promote(
    stage: &Path,
    destination: &Path,
    history: &Path,
    control: &mut impl FnMut(Progress) -> Result<()>,
) -> Result<Value> {
    let stage = root(stage)?;
    let destination = location(destination)?;
    let history = root(history)?;
    disjoint(&[&stage, &destination, &history])?;
    let _lock = lock(&history)?;
    let _destination_lock = destination_lock(&destination)?;
    for entry in fs::read_dir(&history).map_err(io)? {
        let p = entry.map_err(io)?.path();
        if p.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
            s.starts_with("transaction-")
                && (s.ends_with(".json") || (s.ends_with(".tmp") && !s.contains(".interrupted-")))
        }) {
            if p.extension().is_some_and(|s| s == "tmp") {
                return Err(invalid(
                    "Interrupted temporary journal needs operator review",
                ));
            }
            let d = document(&p)?;
            if !["committed", "rolled-back"].contains(&d["phase"].as_str().unwrap_or("")) {
                return Err(invalid(format!(
                    "Recover pending transaction first: {}",
                    p.display()
                )));
            }
        }
    }
    verify_generation(&stage, control)?;
    let new_manifest_sha256 = hash_manifest(&stage)?;
    let old_manifest_sha256 = if destination.exists() {
        verify_generation(&destination, control)?;
        Some(hash_manifest(&destination)?)
    } else {
        None
    };
    // Flush the staged tree before any library rename. The parent directory
    // flushes below persist its location; unsupported filesystems fail closed.
    for tree in std::iter::once(&stage).chain(if old_manifest_sha256.is_some() {
        Some(&destination)
    } else {
        None
    }) {
        let mut dirs = std::collections::BTreeSet::new();
        for p in crate::package::files(tree)? {
            control(Progress {
                stage: Stage::Assemble,
                completed: 0,
                total: 0,
            })?;
            let p = tree.join(p);
            fs::File::open(&p).map_err(io)?.sync_all().map_err(io)?;
            for dir in p
                .parent()
                .unwrap()
                .ancestors()
                .take_while(|p| p.starts_with(tree))
            {
                dirs.insert(dir.to_owned());
            }
        }
        for dir in dirs.iter().rev() {
            sync_dir(dir)?;
        }
    }
    let nonce = format!(
        "{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| invalid(e.to_string()))?
            .as_nanos(),
        std::process::id()
    );
    let archive = history.join(format!("archive-{nonce}"));
    let path = history.join(format!("transaction-{nonce}.json"));
    if archive.exists() || path.exists() {
        return Err(invalid("Transaction name collision"));
    }
    let mut j = Journal {
        schema: "ddi-danoni-publication/1".into(),
        phase: "prepared".into(),
        stage: stage.clone(),
        destination: destination.clone(),
        archive,
        new_manifest_sha256,
        old_manifest_sha256,
    };
    journal(&path, &j)?;
    let action = (|| -> Result<()> {
        control(Progress {
            stage: Stage::Assemble,
            completed: 0,
            total: 2,
        })?;
        if j.old_manifest_sha256.is_some() {
            fs::rename(&destination, &j.archive).map_err(io)?;
            sync_dir(destination.parent().unwrap())?;
            sync_dir(&history)?;
        }
        j.phase = "old-archived".into();
        journal(&path, &j)?;
        control(Progress {
            stage: Stage::Assemble,
            completed: 1,
            total: 2,
        })?;
        // Revalidate after callbacks: cancellation or a concurrent external
        // writer cannot silently change the generation being promoted.
        checked(&stage, &j.new_manifest_sha256, control)?;
        fs::rename(&stage, &destination).map_err(io)?;
        sync_dir(stage.parent().unwrap())?;
        sync_dir(destination.parent().unwrap())?;
        j.phase = "promoted".into();
        journal(&path, &j)?;
        Ok(())
    })();
    // Recovery must run even when the caller's cancellation token is set.
    let result = recover_locked(&path, &mut |_| Ok(()));
    match (action, result) {
        (Ok(()), r) => r,
        (Err(e), Ok(mut r)) => {
            if r["status"] == "committed" {
                r["recoveredError"] = json!(e);
                Ok(r)
            } else {
                Err(e)
            }
        }
        (Err(e), Err(r)) => Err(invalid(format!(
            "{e}; recovery required at {}: {r}",
            path.display()
        ))),
    }
}
