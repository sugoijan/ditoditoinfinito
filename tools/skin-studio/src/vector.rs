//! Native vector documents, clean SVG publication, and game-decoded live pixels.
use super::*;
use std::sync::Arc;
use vectorcraft_engine::{DocState, Session};
use vectorcraft_ui_egui::{Services, VectorcraftApp};

struct Link {
    uid: u64,
    seen: Option<(u64, u64)>,
}

pub(super) struct Workspace {
    pub app: VectorcraftApp,
    pub active: bool,
    links: HashMap<PathBuf, Link>,
    pub retained: HashMap<PathBuf, DocState>,
    pub errors: HashMap<PathBuf, String>,
}

pub(super) struct Change {
    pub source: PathBuf,
    pub result: Result<(String, Rgba), String>,
}

/// Export through VectorCraft, then accept exactly the subset the game renders. Native editing
/// data stays in the project checkpoint, outside runtime SVG. Effects are never silently baked.
pub(super) fn runtime_svg(doc: &vectorcraft_doc::Document) -> Result<(String, Rgba), String> {
    let options = vectorcraft_svg::ExportOptions {
        decimals: 7,
        outline_text: true,
        minify: true,
        styling: vectorcraft_svg::Styling::PresentationAttributes,
        ..Default::default()
    };
    // The mask editor temporarily places a working copy on an isolated layer. Its changes
    // already reach the real mask; that helper layer must never become runtime artwork.
    let doc = doc.without_edit_modes();
    let (svg, warnings) = vectorcraft_svg::export_with_report(&doc, &options);
    if !warnings.is_empty() {
        return Err(warnings.join("\n"));
    }
    let (pixels, _) = ddi_desktop::play::skins::decode_svg(svg.as_bytes())?;
    Ok((svg, pixels))
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            app: VectorcraftApp::new(Session::new(), Services::default()),
            active: true,
            links: HashMap::new(),
            retained: HashMap::new(),
            errors: HashMap::new(),
        }
    }

    pub fn open(&mut self, source: &Path, draft: Option<&String>) -> Result<(), String> {
        if let Some(link) = self.links.get(source)
            && let Some(index) = self
                .app
                .session
                .documents()
                .iter()
                .position(|d| d.uid == link.uid)
        {
            self.app.session.set_active(index);
            self.active = true;
            return Ok(());
        }
        if self.retained.len() >= 128 && !self.retained.contains_key(source) {
            return Err("Project is limited to 128 vector documents".into());
        }
        let state = if let Some(state) = self.retained.get(source) {
            state.clone()
        } else {
            let text = draft
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| files::read_text(source))?;
            ddi_noteskin::svg::check(&text)?;
            let (mut doc, warnings) =
                vectorcraft_svg::import_with_report(&text).map_err(|e| e.to_string())?;
            if !warnings.is_empty() {
                return Err(format!("SVG import: {}", warnings.join("\n")));
            }
            // A noteskin cannot import external images or fonts through this path.
            let _ = runtime_svg(&doc)?;
            doc.title = source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            doc.setup.grid_colors = vectorcraft_doc::setup::GRID_COLOR_PRESETS[2].1;
            DocState::new(doc, None)
        };
        let index = self.app.session.add_document((*state.doc).clone(), None);
        let uid = self
            .app
            .session
            .documents()
            .get(index)
            .ok_or("Could not open SVG")?
            .uid;
        if let Some(slot) = self.app.session.active_mut() {
            *slot = state;
            slot.uid = uid;
            slot.transparency_grid = true;
        }
        self.links
            .insert(source.to_path_buf(), Link { uid, seen: None });
        self.active = true;
        let _ = self.app.run("view.fitArtboard", serde_json::json!({}));
        Ok(())
    }

    pub fn release(&mut self, source: &Path) {
        if let Some(link) = self.links.remove(source)
            && let Some(i) = self
                .app
                .session
                .documents()
                .iter()
                .position(|d| d.uid == link.uid)
        {
            self.app.session.close_document(i);
        }
        self.retained.remove(source);
        self.errors.remove(source);
    }

    pub fn uid(&self, source: &Path) -> Option<u64> {
        self.links.get(source).map(|l| l.uid)
    }

    pub fn open_paths(&self) -> Vec<PathBuf> {
        self.links
            .iter()
            .filter(|(_, link)| {
                self.app
                    .session
                    .documents()
                    .iter()
                    .any(|d| d.uid == link.uid)
            })
            .map(|(p, _)| p.clone())
            .collect()
    }

    pub fn active_path(&self) -> Option<&Path> {
        let uid = self.app.session.active()?.uid;
        self.links
            .iter()
            .find(|(_, link)| link.uid == uid)
            .map(|(p, _)| p.as_path())
    }

    /// Revisions include selection changes. Pointer equality avoids re-exporting unchanged art;
    /// interaction previews change the document snapshot too, before their drag commits.
    pub fn poll(&mut self) -> Vec<Change> {
        let mut changes = Vec::new();
        for (source, link) in &mut self.links {
            let Some(state) = self
                .app
                .session
                .documents()
                .iter()
                .find(|d| d.uid == link.uid)
            else {
                continue;
            };
            if link.seen == Some((state.uid, state.revision)) {
                continue;
            }
            let first = link.seen.is_none();
            link.seen = Some((state.uid, state.revision));
            let same = self
                .retained
                .get(source)
                .is_some_and(|old| Arc::ptr_eq(&old.doc, &state.doc));
            self.retained.insert(source.clone(), state.clone());
            if same && !first {
                continue;
            }
            let result = runtime_svg(&state.doc);
            match &result {
                Ok(_) => {
                    self.errors.remove(source);
                }
                Err(e) => {
                    self.errors.insert(source.clone(), e.clone());
                }
            }
            changes.push(Change {
                source: source.clone(),
                result,
            });
        }
        changes
    }

    pub fn stage_checkpoint(
        &self,
        dir: &Path,
        drafts: &HashMap<PathBuf, String>,
    ) -> Result<Vec<workspace::VectorSource>, String> {
        let generation = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let open = self.open_paths();
        let mut records = Vec::new();
        for (source, state) in &self.retained {
            let file = format!("{}-{generation}.vectorcraft", workspace::key(source));
            let bytes = vectorcraft_format::save_with(&state.doc, &Default::default())
                .map_err(|e| e.to_string())?;
            if bytes.len() > 64 << 20 {
                return Err("Vector source exceeds 64 MiB".into());
            }
            workspace::atomic(&dir.join(&file), &bytes)?;
            records.push(workspace::VectorSource {
                source: source.clone(),
                file,
                svg: match drafts.get(source) {
                    Some(svg) => svg.clone(),
                    None => files::read_text(source)?,
                },
                open: open.contains(source),
            });
        }
        records.sort_by(|a, b| a.source.cmp(&b.source));
        Ok(records)
    }

    pub fn restore(
        &mut self,
        dir: &Path,
        records: &[workspace::VectorSource],
        drafts: &HashMap<PathBuf, String>,
        files: &[files::File],
    ) -> Result<(), String> {
        for record in records {
            if !files.iter().any(|f| {
                f.path == record.source
                    && f.path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
            }) {
                return Err("A vector source no longer belongs to this project".into());
            }
            if record.file.contains(['/', '\\']) || !record.file.ends_with(".vectorcraft") {
                return Err("Invalid vector checkpoint name".into());
            }
            if drafts.get(&record.source).is_some_and(|s| s != &record.svg) {
                continue;
            }
            let path = dir.join(&record.file);
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 << 20 {
                return Err("Vector source exceeds 64 MiB".into());
            }
            let doc = vectorcraft_format::load(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if let Err(error) = runtime_svg(&doc) {
                self.errors.insert(record.source.clone(), error);
            }
            self.retained
                .insert(record.source.clone(), DocState::new(doc, None));
            if record.open {
                self.open(&record.source, drafts.get(&record.source))?;
            }
        }
        Ok(())
    }
}

impl Panel {
    pub(super) fn vector_tick(&mut self) {
        let Some(workspace) = &mut self.vector else {
            return;
        };
        let start = Instant::now();
        let changes = workspace.poll();
        let changed = !changes.is_empty();
        for change in changes {
            self.checkpoint_dirty = true;
            match change.result {
                Ok((svg, pixels)) => {
                    // SVG and pixels represent the same export. Keep raster overrides separate.
                    if let Err(e) = self.editor.set(&change.source, svg.clone()) {
                        self.failure = Some(e);
                        continue;
                    }
                    self.applied_texts.insert(change.source.clone(), svg);
                    for asset in self.assets.iter().filter(|a| a.source == change.source) {
                        if asset.image.size != pixels.size {
                            self.text_reload = Some(Instant::now());
                            continue;
                        }
                        write_rgba(&self.rs.queue, &asset.texture, &pixels);
                        if asset.mipmaps {
                            self.preview.renderer_mut().generate_noteskin_mipmaps(
                                &self.rs.device,
                                &self.rs.queue,
                                &asset.texture,
                            );
                        }
                    }
                    self.thumbnails.remove(&change.source);
                    self.updates += 1;
                    if self.editor.edits == self.applied_texts && workspace.errors.is_empty() {
                        self.failure = None;
                    }
                    self.message = "SVG updated live · vector source preserved".into();
                }
                Err(e) => {
                    self.failure = Some(format!(
                        "SVG is outside the game's supported subset: {e}. Last valid preview retained."
                    ))
                }
            }
        }
        if changed {
            self.update_ms = start.elapsed().as_secs_f64() * 1000.0;
        }
        if self.failure.is_none()
            && let Some((source, error)) = workspace.errors.iter().next()
        {
            self.failure = Some(format!(
                "{}: {error}. Last valid preview retained.",
                source.display()
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dito_svgs_round_trip_through_vectorcraft_and_the_game() {
        let dir = ddi_desktop::images::assets().join("noteskins/dance/Dito");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let source = entry.path();
            if !host::is_svg(&source) {
                continue;
            }
            let before = ddi_desktop::play::skins::decode_image(&source).unwrap().0;
            let text = files::read_text(&source).unwrap();
            let (doc, warnings) = vectorcraft_svg::import_with_report(&text).unwrap();
            assert!(warnings.is_empty(), "{}: {warnings:?}", source.display());
            let (export, after) =
                runtime_svg(&doc).unwrap_or_else(|e| panic!("{}: {e}", source.display()));
            assert_eq!(
                before.size, after.size,
                "logical dimensions must not change"
            );
            let sum: u64 = before
                .data
                .iter()
                .zip(&after.data)
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            let mean = sum as f64 / before.data.len() as f64;
            assert!(
                mean < 0.15,
                "{}: average channel error {mean}",
                source.display()
            );
            assert!(!export.contains("<image"));
            assert!(!export.contains("<metadata"));
            count += 1;
        }
        assert!(count > 10);
    }

    #[test]
    fn vector_anchor_preview_is_live_and_cancel_and_undo_restore_svg() {
        let source = ddi_desktop::images::assets().join("noteskins/dance/Dito/_down receptor.svg");
        let mut workspace = Workspace::new();
        workspace.open(&source, None).unwrap();
        let first = workspace.poll().pop().unwrap().result.unwrap();
        let mut masked = None;
        workspace.app.session.active().unwrap().doc.walk(|n| {
            if masked.is_none() && n.extras().mask.is_some() {
                masked = Some(n.id.0);
            }
        });
        workspace
            .app
            .session
            .execute(
                "transparency.editOpacityMask",
                &serde_json::json!({"id":masked.unwrap()}),
            )
            .unwrap();
        let mask_view = workspace.poll().pop().unwrap().result.unwrap();
        assert!(
            first.1.data == mask_view.1.data,
            "mask editing must not export its temporary working layer"
        );
        let first = mask_view;
        let mut id = None;
        let doc = &workspace.app.session.active().unwrap().doc;
        let layer = doc.mask_edit.unwrap().layer;
        doc.node(layer).unwrap().walk(&mut |n| {
            if id.is_none() && matches!(n.kind, vectorcraft_doc::NodeKind::Path { .. }) {
                id = Some(n.id.0);
            }
        });
        let id = id.unwrap();
        workspace
            .app
            .session
            .begin_interaction("Drag anchor")
            .unwrap();
        workspace
            .app
            .session
            .preview(
                "path.moveAnchor",
                &serde_json::json!({"id":id,"anchor":0,"x":8.0,"y":28.0}),
            )
            .unwrap();
        let moved = workspace.poll().pop().unwrap().result.unwrap();
        assert!(
            moved.1.data != first.1.data,
            "a drag preview updates the exported SVG before commit"
        );
        workspace.app.session.cancel_interaction().unwrap();
        assert_eq!(workspace.poll().pop().unwrap().result.unwrap().0, first.0);
        workspace
            .app
            .session
            .execute(
                "path.moveAnchor",
                &serde_json::json!({"id":id,"anchor":0,"x":8.0,"y":28.0}),
            )
            .unwrap();
        workspace.poll();
        workspace
            .app
            .session
            .execute("edit.undo", &serde_json::json!({}))
            .unwrap();
        assert_eq!(workspace.poll().pop().unwrap().result.unwrap().0, first.0);
    }
}
