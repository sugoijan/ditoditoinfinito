//! Checkpoints for independent documents opened through native File menus. They are separate
//! from noteskin overrides, but closing the window must retain them in either editor.
use super::*;
#[derive(serde::Serialize, serde::Deserialize)]
struct Record {
    vector: bool,
    id: u64,
    revision: u64,
    path: Option<String>,
    file: String,
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct State {
    records: Vec<Record>,
    vector_active: bool,
}
fn read(home: &Path) -> Result<State, String> {
    let p = home.join("references/session.json");
    if !p.exists() {
        return Ok(State::default());
    }
    if std::fs::metadata(&p).map_err(|e| e.to_string())?.len() > 1 << 20 {
        return Err("Reference manifest exceeds 1 MiB".into());
    }
    let state: State = serde_json::from_slice(&std::fs::read(p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if state.records.len() > 128
        || state.records.iter().any(|r| {
            r.file.contains(['/', '\\'])
                || r.file.starts_with('.')
                || !(r.file.ends_with(".pcraft") || r.file.ends_with(".vectorcraft"))
        })
    {
        return Err("Invalid reference session".into());
    }
    Ok(state)
}
pub(super) fn save(app: &host::SkinCraft) -> Result<(), String> {
    let home = app.panel.home.as_ref().ok_or("No project home")?;
    let previous = read(home)?;
    let dir = home.join("references");
    let generation = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let mut state = State {
        vector_active: app.panel.vector_active(),
        ..Default::default()
    };
    let mut total = 0u64;
    let mut stage = |vector: bool,
                     id: u64,
                     revision: u64,
                     path: Option<String>,
                     encode: &mut dyn FnMut() -> Result<Vec<u8>, String>|
     -> Result<(), String> {
        if state.records.len() >= 128 {
            return Err("Too many reference documents".into());
        }
        let existing = previous.records.iter().find(|r| {
            r.vector == vector
                && r.id == id
                && r.revision == revision
                && dir.join(&r.file).is_file()
        });
        let file = if let Some(old) = existing {
            old.file.clone()
        } else {
            let bytes = encode()?;
            if bytes.len() > 256 << 20 {
                return Err("Reference checkpoint exceeds 256 MiB".into());
            }
            let name = format!(
                "{}-{id}-{generation}.{}",
                if vector { "vector" } else { "raster" },
                if vector { "vectorcraft" } else { "pcraft" }
            );
            workspace::atomic(&dir.join(&name), &bytes)?;
            name
        };
        total = total.saturating_add(
            std::fs::metadata(dir.join(&file))
                .map_err(|e| e.to_string())?
                .len(),
        );
        if total > 512 << 20 {
            return Err("Reference checkpoints exceed 512 MiB".into());
        }
        state.records.push(Record {
            vector,
            id,
            revision,
            path,
            file,
        });
        Ok(())
    };
    for d in app
        .photo
        .session
        .documents()
        .iter()
        .filter(|d| !app.panel.bindings.values().any(|b| b.doc == d.doc.id))
    {
        stage(false, d.doc.id.0, d.revision, d.path.clone(), &mut || {
            photocraft_format::save_to_bytes(&d.doc, &Default::default()).map_err(|e| e.to_string())
        })?;
    }
    if let Some(v) = &app.panel.vector {
        let ids: Vec<_> = v.open_paths().iter().filter_map(|p| v.uid(p)).collect();
        for d in v
            .app
            .session
            .documents()
            .iter()
            .filter(|d| !ids.contains(&d.uid))
        {
            stage(true, d.uid, d.revision, d.path.clone(), &mut || {
                vectorcraft_format::save_with(&d.doc, &Default::default())
                    .map_err(|e| e.to_string())
            })?;
        }
    }
    workspace::atomic(
        &dir.join("session.json"),
        &serde_json::to_vec(&state).map_err(|e| e.to_string())?,
    )?;
    for old in previous.records {
        if !state.records.iter().any(|r| r.file == old.file) {
            let _ = std::fs::remove_file(dir.join(old.file));
        }
    }
    Ok(())
}
pub(super) fn restore(app: &mut host::SkinCraft) -> Result<(), String> {
    let home = app.panel.home.as_ref().ok_or("No project home")?;
    if !home.join("references/session.json").exists() {
        return Ok(());
    }
    let state = read(home)?;
    let mut total = 0u64;
    for record in state.records {
        let file = home.join("references").join(record.file);
        let size = std::fs::metadata(&file).map_err(|e| e.to_string())?.len();
        total = total.saturating_add(size);
        if size > 256 << 20 || total > 512 << 20 {
            return Err("Reference checkpoints exceed the project limit".into());
        }
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        if record.vector {
            let doc = vectorcraft_format::load(&bytes).map_err(|e| e.to_string())?;
            let v = app.panel.vector.as_mut().ok_or("No vector editor")?;
            v.app.session.add_document(doc, record.path);
        } else {
            let options = photocraft_format::LoadOptions {
                max_total_bytes: 256 << 20,
                ..Default::default()
            };
            let doc = photocraft_format::load_from_bytes_with(&bytes, &options)
                .map_err(|e| e.to_string())?;
            app.photo.session.add_document(doc, record.path);
        }
    }
    if let Some(v) = &mut app.panel.vector {
        v.active = state.vector_active;
    }
    Ok(())
}
