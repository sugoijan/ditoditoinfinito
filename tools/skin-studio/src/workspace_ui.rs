use super::*;

impl Panel {
    pub(super) fn candidate_dir(&self) -> PathBuf {
        let tick = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        self.home
            .clone()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("preview-{}-{tick}", std::process::id()))
    }

    pub(super) fn restore_workspace(&mut self) -> Result<(), String> {
        let Some(home) = &self.home else {
            return Ok(());
        };
        let project = self.project.as_ref().ok_or("No project")?;
        let dir = workspace::dir(home, &project.dir);
        self.workspace_dir = Some(dir.clone());
        self.checkpoint_dirty = false;
        let Some(state) = workspace::load(&dir)? else {
            return Ok(());
        };
        let mut total = 0u64;
        for (source, name) in state.drawings {
            if !self.files.iter().any(|f| f.path == source && f.image) {
                return Err(format!(
                    "Saved artwork no longer belongs to the project: {}",
                    source.display()
                ));
            }
            let doc = workspace::drawing(&dir, &name)?;
            total = total.saturating_add(doc.doc.size.area());
            if total > 64 << 20 {
                return Err("Saved artwork exceeds the 64 megapixel project limit".into());
            }
            let size = self
                .assets
                .iter()
                .find(|a| a.source == source)
                .map(|a| a.image.size)
                .unwrap_or([doc.doc.size.width, doc.doc.size.height]);
            self.edits
                .insert(source.clone(), composite(&doc.doc, size)?);
            self.sizes.insert(source.clone(), size);
            self.drawings.insert(source, doc);
        }
        for path in state.texts.keys() {
            if !self.files.iter().any(|f| &f.path == path && f.text) {
                return Err(format!(
                    "Saved text no longer belongs to the project: {}",
                    path.display()
                ));
            }
        }
        self.editor.edits = state.texts;
        #[cfg(feature = "skincraft")]
        if let Some(vector) = &mut self.vector {
            vector.restore(&dir, &state.vectors, &self.editor.edits, &self.files)?;
        }

        for path in &state.text_tabs {
            if !self.files.iter().any(|f| &f.path == path && f.text) {
                return Err("Saved text tab no longer belongs to this project".into());
            }
            self.editor.open(path)?;
        }
        self.editor.active = state
            .active_text
            .filter(|p| self.editor.tabs.contains(p))
            .or_else(|| self.editor.tabs.last().cloned());
        self.editor.visible = !self.editor.tabs.is_empty();
        self.pending_open = state.open;
        if !self.editor.edits.is_empty() {
            self.text_reload = Some(Instant::now() - std::time::Duration::from_secs(1));
        }
        // Closed documents still feed their saved pixels into the preview.
        for asset in &self.assets {
            if let Some(image) = self.edits.get(&asset.source) {
                write_rgba(&self.rs.queue, &asset.texture, image);
                if asset.mipmaps {
                    self.preview.renderer_mut().generate_noteskin_mipmaps(
                        &self.rs.device,
                        &self.rs.queue,
                        &asset.texture,
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn checkpoint(&mut self, app: &PhotocraftApp) -> Result<(), String> {
        let Some(home) = &self.home else {
            return Ok(());
        };
        let project = self.project.as_ref().ok_or("Load a skin first")?;
        let dir = workspace::dir(home, &project.dir);
        // A save can follow a command in the same frame, before the next preview update.
        for (source, binding) in &self.bindings {
            if let Some(doc) = app
                .session
                .documents()
                .iter()
                .find(|d| d.doc.id == binding.doc)
            {
                self.drawings.insert(source.clone(), doc.clone());
            }
        }
        let mut open: Vec<_> = self.bindings.keys().cloned().collect();
        open.sort();
        #[cfg(feature = "skincraft")]
        let vectors = match &self.vector {
            Some(v) => v.stage_checkpoint(&dir, &self.editor.edits)?,
            None => workspace::load(&dir)?
                .map(|s| s.vectors)
                .unwrap_or_default(),
        };
        #[cfg(not(feature = "skincraft"))]
        workspace::save(
            &dir,
            &self.drawings,
            &self.editor.edits,
            open,
            self.editor.tabs.clone(),
            self.editor.active.clone(),
        )?;
        #[cfg(feature = "skincraft")]
        workspace::save_full(
            &dir,
            &self.drawings,
            &self.editor.edits,
            open,
            self.editor.tabs.clone(),
            self.editor.active.clone(),
            vectors,
        )?;
        self.workspace_dir = Some(dir);
        self.checkpoint_dirty = false;
        self.message = "Project saved: editable artwork, closed tabs and text drafts. Export skin writes game files.".into();
        Ok(())
    }

    fn has_drawing_override(&self, source: &Path) -> bool {
        #[cfg(feature = "skincraft")]
        if self
            .vector
            .as_ref()
            .is_some_and(|v| v.retained.contains_key(source))
        {
            return true;
        }
        self.drawings.contains_key(source)
    }

    pub(super) fn open_file(
        &mut self,
        app: &mut PhotocraftApp,
        source: &Path,
    ) -> Result<(), String> {
        #[cfg(feature = "skincraft")]
        if host::is_svg(source)
            && let Some(vector) = &mut self.vector
        {
            if self.drawings.contains_key(source) {
                return Err(
                    "This SVG has a raster override. Choose Use SVG source to switch editors"
                        .into(),
                );
            }
            if !self.files.iter().any(|f| f.path == source && f.image) {
                return Err("Image does not belong to the current noteskin".into());
            }
            vector.open(source, self.editor.edits.get(source))?;
            self.selected_file = Some(source.to_path_buf());
            if let Some(index) = self.assets.iter().position(|a| a.source == source) {
                self.selected = index;
            }
            self.editor.visible = false;
            self.checkpoint_dirty = true;
            return Ok(());
        }
        #[cfg(feature = "skincraft")]
        if let Some(vector) = &mut self.vector {
            vector.active = false;
        }
        self.open_image(app, source).map(|_| ())
    }

    pub(super) fn open_image(
        &mut self,
        app: &mut PhotocraftApp,
        source: &Path,
    ) -> Result<DocId, String> {
        if !self.files.iter().any(|f| f.path == source && f.image) {
            return Err("This image does not belong to the current noteskin".into());
        }
        if self.editor.edits.contains_key(source) {
            return Err("This SVG has source edits. Use its source tab, or revert the source edits before painting it.".into());
        }
        if let Some(binding) = self.bindings.get(source)
            && let Some(index) = app
                .session
                .documents()
                .iter()
                .position(|s| s.doc.id == binding.doc)
        {
            app.session.set_active(index);
            self.editor.visible = false;
            return Ok(binding.doc);
        }
        if self.drawings.len() >= 128 && !self.drawings.contains_key(source) {
            return Err("Project is limited to 128 layered images".into());
        }
        let state = if let Some(state) = self.drawings.get(source) {
            state.clone()
        } else {
            if std::fs::metadata(source).map_err(|e| e.to_string())?.len() > 32 << 20 {
                return Err("Image exceeds 32 MiB".into());
            }
            let image = if let Some(image) = self.edits.get(source) {
                image.clone()
            } else if let Some(asset) = self.assets.iter().find(|a| a.source == source) {
                asset.image.clone()
            } else {
                ddi_desktop::play::skins::decode_image(source)?.0
            };
            let pixels = u64::from(image.size[0]) * u64::from(image.size[1]);
            if pixels > 4 << 20 {
                return Err("Live images must fit within 4 megapixels".into());
            }
            if self
                .drawings
                .values()
                .map(|d| d.doc.size.area())
                .sum::<u64>()
                .saturating_add(pixels)
                > 64 << 20
            {
                return Err("Project artwork exceeds 64 megapixels".into());
            }
            self.sizes.insert(source.to_path_buf(), image.size);
            let imported = photocraft_io::import(
                &format!(
                    "{} — artwork.png",
                    source.file_name().unwrap_or_default().to_string_lossy()
                ),
                &project::png_bytes(&image)?,
            )
            .map_err(|e| e.to_string())?;
            photocraft_engine::DocState::new(imported.document, None)
        };
        let index = app
            .session
            .add_document((*state.doc).clone(), state.path.clone());
        let admitted = app
            .session
            .active()
            .ok_or("Could not open document")?
            .doc
            .clone();
        if let Some(slot) = app.session.active_mut() {
            *slot = state;
            slot.doc = admitted;
        }
        let admitted = app
            .session
            .documents()
            .get(index)
            .ok_or("Could not open document")?;
        let doc = admitted.doc.id;
        self.drawings.insert(source.to_path_buf(), admitted.clone());
        self.bindings
            .insert(source.to_path_buf(), Binding { doc, seen: None });
        self.selected_file = Some(source.to_path_buf());
        if let Some(index) = self.assets.iter().position(|a| a.source == source) {
            self.selected = index;
        }
        self.checkpoint_dirty = true;
        self.editor.visible = false;
        self.message =
            "Drawing is live. Save project keeps every image and its layers together.".into();
        Ok(doc)
    }

    pub(super) fn workspace_tick(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context) {
        let visible = self.editor.visible;
        let was_restoring = !self.pending_open.is_empty();
        if !self.pending_open.is_empty() {
            let source = self.pending_open.remove(0);
            if let Err(e) = self.open_file(app, &source) {
                self.failure = Some(e);
            }
        }
        if was_restoring {
            self.editor.visible = visible;
            ctx.request_repaint();
        }
        #[cfg(feature = "skincraft")]
        if let Some(v) = &self.vector {
            self.editor.read_only = v.retained.keys().cloned().collect();
        }
        let tabs_before = (self.editor.tabs.clone(), self.editor.active.clone());
        if self.editor.show(ctx, self.failure.as_deref()) {
            self.text_reload = Some(Instant::now());
            self.checkpoint_dirty = true;
        }
        if tabs_before != (self.editor.tabs.clone(), self.editor.active.clone()) {
            self.checkpoint_dirty = true;
        }
        if self
            .text_reload
            .is_some_and(|at| at.elapsed().as_millis() >= 450)
        {
            self.text_reload = None;
            let validation = self.editor.edits.iter().try_for_each(|(path,text)| {
                if self.drawings.contains_key(path) { return Err("An SVG has both raster and source edits. Select it in Noteskin Files and choose Use SVG source.".into()); }
                if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) { ddi_noteskin::svg::check(text).map(|_| ())?; }
                Ok::<_, String>(())
            });
            match validation.and_then(|()| self.load()) {
                Ok(()) => {
                    self.message =
                        "Text changes loaded. Preview uses the current project drafts.".into();
                    self.thumbnails.clear();
                }
                Err(e) => self.failure = Some(e),
            }
        }
        if self.text_reload.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn thumbnail(
        &mut self,
        path: &Path,
        ctx: &egui::Context,
        decode: &mut bool,
    ) -> Option<egui::TextureId> {
        if let Some(texture) = self.thumbnails.get(path) {
            return Some(texture.id());
        }
        if self.thumbnails.len() >= 128 || self.thumbnail_failed.contains(path) || !*decode {
            return None;
        }
        *decode = false;
        self.thumbnail_failed.insert(path.to_path_buf());
        let image = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
            && self.editor.edits.contains_key(path)
        {
            ddi_desktop::play::skins::decode_svg(self.editor.edits.get(path)?.as_bytes())
                .ok()?
                .0
        } else if let Some(image) = self.edits.get(path) {
            image.clone()
        } else if let Some(asset) = self.assets.iter().find(|a| a.source == path) {
            asset.image.clone()
        } else {
            if std::fs::metadata(path).ok()?.len() > 32 << 20 {
                return None;
            }
            ddi_desktop::play::skins::decode_image(path).ok()?.0
        };
        let source = image::RgbaImage::from_raw(image.size[0], image.size[1], image.data)?;
        let thumb = image::imageops::thumbnail(&source, 48, 48);
        let size = [thumb.width() as usize, thumb.height() as usize];
        let texture = ctx.load_texture(
            path.display().to_string(),
            egui::ColorImage::from_rgba_unmultiplied(size, &thumb.into_raw()),
            Default::default(),
        );
        let id = texture.id();
        self.thumbnails.insert(path.to_path_buf(), texture);
        self.thumbnail_failed.remove(path);
        Some(id)
    }

    pub(super) fn workspace_browser(&mut self, app: &mut PhotocraftApp, ui: &mut egui::Ui) {
        if self.files_collapsed {
            egui::Panel::left("noteskin-files-collapsed")
                .exact_size(30.0)
                .show(ui, |ui| {
                    if ui
                        .button("▶")
                        .on_hover_text("Show noteskin files")
                        .clicked()
                    {
                        self.files_collapsed = false;
                    }
                });
            return;
        }
        egui::Panel::left("noteskin-files").default_size(230.0).size_range(175.0..=380.0).resizable(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Noteskin Files");
                if ui.small_button("◀").on_hover_text("Collapse files").clicked() { self.files_collapsed = true; }
            });
            ui.horizontal(|ui| {
                if ui.button("Save project").clicked() {
                    match self.save_project(app) { Ok(()) => self.message = "Project saved".into(), Err(e) => self.failure = Some(e) }
                }
                if ui.button("Text tabs").clicked() { self.editor.visible = true; }
            });
            if ui.button("Open all images").on_hover_text("Open images progressively, within the project's memory limits").clicked() {
                self.pending_open = self.files.iter().filter(|f| f.image).map(|f| f.path.clone()).collect();
            }
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Find files…"));
            egui::ComboBox::from_id_salt("inspect-element").selected_text("Inspect element…").show_ui(ui,|ui| {
                for (label,filter) in [("All files",""),("Tap notes","tap"),("Receptors","receptor"),("Holds","hold"),("Rolls","roll"),("Mines","mine"),("Explosions","explosion"),("Onigiri","onigiri"),("Loader scripts",".lua"),("Metrics",".ini")] {
                    if ui.selectable_label(false,label).clicked() { self.search = filter.into(); }
                }
            });
            let query = self.search.to_lowercase();
            let entries: Vec<_> = self.files.iter().filter(|f| format!("{} {}",f.owner,f.name).to_lowercase().contains(&query)).cloned().collect();
            let mut decode = true;
            let mut action = None;
            let mut owner = String::new();
            egui::ScrollArea::vertical().id_salt("noteskin-file-list").max_height((ui.available_height()-190.0).max(100.0)).show(ui,|ui| {
                for file in entries {
                    if owner != file.owner { owner = file.owner.clone(); ui.separator(); ui.strong(format!("{}{}",owner,if file.inherited {" · inherited"} else {""})); }
                    let rect = ui.available_rect_before_wrap();
                    let visible = rect.top() < ui.clip_rect().bottom() && rect.top()+34.0 > ui.clip_rect().top();
                    ui.horizontal(|ui| {
                        if file.image {
                            if visible && let Some(id) = self.thumbnail(&file.path,ui.ctx(),&mut decode) { ui.image((id,egui::vec2(28.0,28.0))); }
                            else { ui.allocate_space(egui::vec2(28.0,28.0)); }
                        }
                        let modified = self.drawings.contains_key(&file.path) || self.editor.edits.contains_key(&file.path);
                        let label = format!("{}{}",file.name,if modified {" •"} else {""});
                        let response = ui.add(egui::Button::selectable(self.selected_file.as_ref() == Some(&file.path),label).truncate()).on_hover_text(file.path.display().to_string());
                        if response.clicked() { self.selected_file = Some(file.path.clone()); }
                        if response.double_clicked() { action = Some((file.clone(),false)); }
                    });
                }
            });
            ui.separator();
            if let Some(file) = self.files.iter().find(|f| Some(&f.path) == self.selected_file.as_ref()).cloned() {
                ui.add(egui::Label::new(&file.name).truncate());
                ui.small(format!("From {}{}",file.owner,if file.inherited {" · edits become project overrides"} else {""}));
                if file.image && ui.button("Open image").clicked() { action = Some((file.clone(),false)); }
                if file.text && ui.button("Edit source text").clicked() { action = Some((file.clone(),true)); }
                if file.image && file.text && self.has_drawing_override(&file.path) && ui.button("Use SVG source").on_hover_text("Remove this project's raster override; the drawing stays open as an independent document").clicked() {
                    #[cfg(feature="skincraft")]
                    if let Some(v)=&mut self.vector {v.release(&file.path);}
                    self.bindings.remove(&file.path); self.drawings.remove(&file.path); self.edits.remove(&file.path); self.sizes.remove(&file.path);
                    self.text_reload = Some(Instant::now()); self.checkpoint_dirty = true;
                }
                if let Some(i) = self.assets.iter().position(|a| a.source == file.path) { self.selected = i; ui.small("Used in this preview"); }
                else { ui.small("Not loaded by the current layout / actor selection"); }
                if !file.image && !file.text { ui.small("Binary resource: preserved when exporting"); }
            } else { ui.small("Double-click a file to edit. Images open in PhotoCraft tabs; scripts open in the text workspace."); }
            if let Some((file,source)) = action {
                let result = if file.image && !source { self.open_file(app,&file.path) }
                    else if file.text { self.editor.open(&file.path) }
                    else { Err("This binary resource has no editor".into()) };
                if let Err(e) = result { self.failure = Some(e); }
                self.checkpoint_dirty = true;
            }
            ui.small(format!("{} files · {} images in project",self.files.len(),self.drawings.len()));
        });
    }
}

impl Panel {
    pub(super) fn element_inspector(&mut self, app: &mut PhotocraftApp, ui: &mut egui::Ui) {
        ui.collapsing("Inspect element and its files", |ui| {
            if let Some(layout) = Layout::builtin(&self.options.layout) {
                egui::ComboBox::from_id_salt("inspect-lane")
                    .selected_text(
                        layout
                            .lanes
                            .get(self.inspect_lane)
                            .map(|l| l.name.as_str())
                            .unwrap_or("Choose lane"),
                    )
                    .show_ui(ui, |ui| {
                        for (i, lane) in layout.lanes.iter().enumerate() {
                            ui.selectable_value(&mut self.inspect_lane, i, &lane.name);
                        }
                    });
            }
            egui::ComboBox::from_id_salt("inspect-part")
                .selected_text(&self.inspect_part)
                .show_ui(ui, |ui| {
                    for part in [
                        "Tap Note",
                        "Receptor",
                        "Explosion",
                        "Tap Lift",
                        "Tap Fake",
                        "Tap Mine",
                        "Hold Head Active",
                        "Hold Head Inactive",
                        "Hold Head Dropped",
                        "Hold Body Active",
                        "Hold Body Inactive",
                        "Hold Body Dropped",
                        "Hold Tail Active",
                        "Hold Tail Dropped",
                        "Hold Topcap Active",
                        "Hold Bottomcap Active",
                        "Roll Head Active",
                        "Roll Body Active",
                        "Roll Body Inactive",
                        "Roll Head Dropped",
                        "Roll Tail Dropped",
                        "Roll Body Dropped",
                    ] {
                        ui.selectable_value(&mut self.inspect_part, part.into(), part);
                    }
                });
            let keys = self
                .preview
                .noteskin()
                .map(|p| p.element_texture_keys(self.inspect_lane, &self.inspect_part))
                .unwrap_or_default();
            let sources: Vec<_> = keys
                .iter()
                .filter_map(|k| self.project.as_ref().and_then(|p| p.source(k)))
                .collect();
            if sources.is_empty() {
                ui.small("No image texture: this element is blank or drawn procedurally.");
            }
            for source in sources {
                if ui
                    .link(source.file_name().unwrap_or_default().to_string_lossy())
                    .on_hover_text(source.display().to_string())
                    .clicked()
                    && let Err(e) = self.open_file(app, &source)
                {
                    self.failure = Some(e);
                }
            }
            ui.small("Related scripts and metrics (custom Lua can load additional files):");
            let related: Vec<_> = self
                .files
                .iter()
                .filter(|f| {
                    f.text
                        && (f
                            .name
                            .to_lowercase()
                            .contains(&self.inspect_part.to_lowercase())
                            || f.name.eq_ignore_ascii_case("NoteSkin.lua")
                            || f.name.eq_ignore_ascii_case("metrics.ini"))
                })
                .cloned()
                .collect();
            for file in related {
                if ui.link(format!("{}/{}", file.owner, file.name)).clicked() {
                    if let Err(e) = self.editor.open(&file.path) {
                        self.failure = Some(e);
                    }
                    self.checkpoint_dirty = true;
                }
            }
        });
    }
}
