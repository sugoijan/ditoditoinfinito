//! Layered project checkpoints. Runtime assets and upstream sources are never overwritten.
use photocraft_engine::DocState;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    #[serde(default)]
    pub drawings: Vec<(PathBuf, String)>,
    #[serde(default)]
    pub open: Vec<PathBuf>,
    #[serde(default)]
    pub texts: HashMap<PathBuf, String>,
    #[serde(default)]
    pub text_tabs: Vec<PathBuf>,
    #[serde(default)]
    pub active_text: Option<PathBuf>,
    #[serde(default)]
    pub revisions: HashMap<PathBuf, (u64, u64)>,
    #[serde(default)]
    pub vectors: Vec<VectorSource>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct VectorSource {
    pub source: PathBuf,
    pub file: String,
    pub svg: String,
    pub open: bool,
}

pub fn key(path: &Path) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hash);
    format!("{:016x}", hash.finish())
}

pub fn dir(home: &Path, source: &Path) -> PathBuf {
    home.join("projects").join(key(source))
}

pub fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("File needs a parent")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = path.with_extension("writing");
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}

/// Write changed layered documents before committing the manifest. Generation filenames keep
/// the previous manifest usable if a write fails halfway through the checkpoint.
#[cfg(any(not(feature = "skincraft"), test))]
pub fn save(
    dir: &Path,
    docs: &HashMap<PathBuf, DocState>,
    texts: &HashMap<PathBuf, String>,
    open: Vec<PathBuf>,
    text_tabs: Vec<PathBuf>,
    active_text: Option<PathBuf>,
) -> Result<(), String> {
    let previous = load(dir)?.unwrap_or_default();
    save_full(
        dir,
        docs,
        texts,
        open,
        text_tabs,
        active_text,
        previous.vectors,
    )
}

pub fn save_full(
    dir: &Path,
    docs: &HashMap<PathBuf, DocState>,
    texts: &HashMap<PathBuf, String>,
    open: Vec<PathBuf>,
    text_tabs: Vec<PathBuf>,
    active_text: Option<PathBuf>,
    vectors: Vec<VectorSource>,
) -> Result<(), String> {
    let generation = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let previous = load(dir)?.unwrap_or_default();
    let mut state = State {
        texts: texts.clone(),
        open,
        text_tabs,
        active_text,
        vectors,
        ..Default::default()
    };
    for (source, doc) in docs {
        state
            .revisions
            .insert(source.clone(), (doc.doc.id.0, doc.revision));
        if previous.revisions.get(source) == Some(&(doc.doc.id.0, doc.revision))
            && let Some((_, name)) = previous.drawings.iter().find(|(p, _)| p == source)
            && dir.join(name).is_file()
        {
            state.drawings.push((source.clone(), name.clone()));
            continue;
        }
        let name = format!("{}-{generation}.pcraft", key(source));
        let bytes = photocraft_format::save_to_bytes(&doc.doc, &Default::default())
            .map_err(|e| e.to_string())?;
        atomic(&dir.join(&name), &bytes)?;
        state.drawings.push((source.clone(), name));
    }
    state.drawings.sort();
    let manifest = serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?;
    if manifest.len() > 32 << 20 {
        return Err("Project text drafts exceed the 32 MiB manifest limit".into());
    }
    atomic(&dir.join("workspace.json"), &manifest)?;
    // Only files owned by the previous committed manifest may be removed.
    for (_, name) in previous.drawings {
        if !state.drawings.iter().any(|(_, keep)| keep == &name) {
            let _ = std::fs::remove_file(dir.join(name));
        }
    }
    for record in previous.vectors {
        if !state.vectors.iter().any(|v| v.file == record.file)
            && !record.file.contains(['/', '\\'])
        {
            let _ = std::fs::remove_file(dir.join(record.file));
        }
    }
    Ok(())
}

pub fn load(dir: &Path) -> Result<Option<State>, String> {
    let path = dir.join("workspace.json");
    if !path.exists() {
        return Ok(None);
    }
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 32 << 20 {
        return Err("Project manifest exceeds 32 MiB".into());
    }
    let state: State = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if state.drawings.len() > 128
        || state.vectors.len() > 128
        || state.vectors.iter().any(|v| v.svg.len() > 1 << 20)
        || state.texts.len() > 256
        || state.texts.values().any(|s| s.len() > 1 << 20)
    {
        return Err("Project has too many or oversized files".into());
    }
    if state.drawings.iter().any(|(_, name)| {
        !name.ends_with(".pcraft") || name.contains(['/', '\\']) || name.starts_with('.')
    }) {
        return Err("Invalid layered source filename".into());
    }
    if state.vectors.iter().any(|v| {
        !v.file.ends_with(".vectorcraft") || v.file.contains(['/', '\\']) || v.file.starts_with('.')
    }) {
        return Err("Invalid vector source filename".into());
    }
    Ok(Some(state))
}

pub fn drawing(dir: &Path, name: &str) -> Result<DocState, String> {
    let path = dir.join(name);
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 256 << 20 {
        return Err("Layered source exceeds 256 MiB".into());
    }
    let opts = photocraft_format::LoadOptions {
        max_canvas_side: 4096,
        max_canvas_pixels: 4 << 20,
        max_total_bytes: 256 << 20,
        ..Default::default()
    };
    let doc = photocraft_format::load_path_with(&path, &opts).map_err(|e| e.to_string())?;
    Ok(DocState::new(doc, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_preserve_layers_and_rebinding_at_the_same_revision() {
        let root = crate::project::tests::fixture("workspace-storage");
        let dir = root.join("sources");
        let source = root.join("dance/Test/Down Tap Note.png");
        let mut session = photocraft_engine::Session::new();
        session
            .execute("file.new", serde_json::json!({"width":32,"height":48}))
            .unwrap();
        session
            .execute("layer.new.layer", serde_json::json!({"name":"Ink"}))
            .unwrap();
        let doc = session.active().unwrap().clone();
        let docs = HashMap::from([(source.clone(), doc.clone())]);
        let texts = HashMap::from([(
            root.join("dance/Test/metrics.ini"),
            "[Global]\n; draft".into(),
        )]);
        save(&dir, &docs, &texts, vec![], vec![], None).unwrap();
        let a = load(&dir).unwrap().unwrap();
        let restored = drawing(&dir, &a.drawings[0].1).unwrap();
        assert_eq!(restored.doc.layers.len(), doc.doc.layers.len());
        assert_eq!(restored.doc.size, doc.doc.size);
        assert_eq!(a.texts, texts);
        save(&dir, &docs, &texts, vec![], vec![], None).unwrap();
        assert_eq!(
            load(&dir).unwrap().unwrap().drawings,
            a.drawings,
            "unchanged layered documents are not encoded again"
        );
        session
            .execute("file.new", serde_json::json!({"width":16,"height":16}))
            .unwrap();
        let mut replacement = session.active().unwrap().clone();
        replacement.revision = doc.revision;
        save(
            &dir,
            &HashMap::from([(source, replacement)]),
            &texts,
            vec![],
            vec![],
            None,
        )
        .unwrap();
        let b = load(&dir).unwrap().unwrap();
        assert_ne!(
            a.drawings, b.drawings,
            "a rebound document with the same revision needs a fresh snapshot"
        );
        assert_eq!(drawing(&dir, &b.drawings[0].1).unwrap().doc.size.width, 16);
        assert!(
            !dir.join(&a.drawings[0].1).exists(),
            "only obsolete generated snapshots are cleaned up"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
