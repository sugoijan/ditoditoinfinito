//! A single event loop hosting the two native editors and the persistent noteskin workspace.
use super::*;
use eframe::App;
use photocraft_ui_egui::Services;

pub struct SkinCraft {
    pub(super) photo: PhotocraftApp,
    pub(super) panel: Panel,
    fonts_ready: bool,
    pub(super) native: Option<native::Runtime>,
    pub(super) control: Option<control::Server>,
    pub(super) captures: Vec<control::Capture>,
    pub(super) capture_id: u64,
    references_at: Instant,
}

impl SkinCraft {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let rs = cc
            .wgpu_render_state
            .clone()
            .ok_or("SkinCraft needs a GPU render state")?;
        let home = std::env::var_os("DDI_STUDIO_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                #[cfg(target_os = "macos")]
                {
                    std::env::var_os("HOME")
                        .map(|h| PathBuf::from(h).join("Library/Application Support/SkinCraft"))
                }
                #[cfg(target_os = "windows")]
                {
                    std::env::var_os("APPDATA").map(|h| PathBuf::from(h).join("SkinCraft"))
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    std::env::var_os("XDG_CONFIG_HOME")
                        .map(PathBuf::from)
                        .or_else(|| {
                            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
                        })
                        .map(|p| p.join("skincraft"))
                }
            })
            .ok_or("Could not find a project home; set DDI_STUDIO_HOME")?;
        let mut app = Self::from_render_state(rs, home.clone())?;
        app.native = Some(native::Runtime::install(&mut app, cc)?);
        app.control = Some(control::Server::start(&home, cc.egui_ctx.clone())?);
        Ok(app)
    }

    pub fn from_render_state(rs: RenderState, home: PathBuf) -> Result<Self, String> {
        let mut photo = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
        photo.set_wgpu(rs.clone());
        photo.ui.panels.menu_bar = false;
        let mut panel = Panel::at_home(rs, home);
        panel.vector = Some(vector::Workspace::new());
        panel.load()?;
        let has_sources = panel
            .vector
            .as_ref()
            .is_some_and(|v| !v.retained.is_empty())
            || !panel.drawings.is_empty()
            || !panel.pending_open.is_empty();
        if !has_sources {
            let first = panel
                .files
                .iter()
                .find(|f| f.name == "_down receptor.svg")
                .or_else(|| panel.files.iter().find(|f| is_svg(&f.path)))
                .map(|f| f.path.clone());
            if let Some(source) = first {
                panel.open_file(&mut photo, &source)?;
            }
        }
        let mut app = Self {
            photo,
            panel,
            fonts_ready: false,
            native: None,
            control: None,
            captures: Vec::new(),
            capture_id: 0,
            references_at: Instant::now(),
        };
        references::restore(&mut app)?;
        Ok(app)
    }

    /// State for embedded hosts and smoke tests, without exposing either editor's internals.
    pub fn inspect(&self) -> serde_json::Value {
        self.panel.inspect()
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("skincraft-tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("SkinCraft");
                ui.separator();
                if ui.button("Open file…").clicked() {
                    self.ask(native::Action::Open, ui.ctx());
                }
                if ui.button("Open noteskin…").clicked() {
                    self.ask(native::Action::Skin, ui.ctx());
                }
                if ui
                    .button("Export skin…")
                    .on_hover_text("Choose a parent folder; creates a new noteskin export there")
                    .clicked()
                {
                    self.ask(native::Action::Export, ui.ctx());
                }
                if ui
                    .add_enabled(
                        self.panel.editor.active.is_some(),
                        egui::Button::new("Save text copy…"),
                    )
                    .clicked()
                {
                    self.ask(native::Action::TextCopy, ui.ctx());
                }
                ui.separator();
                if ui
                    .selectable_label(self.panel.vector_active(), "Vector")
                    .clicked()
                    && let Some(v) = &mut self.panel.vector
                {
                    v.active = true;
                }
                if ui
                    .selectable_label(!self.panel.vector_active(), "Raster")
                    .clicked()
                    && let Some(v) = &mut self.panel.vector
                {
                    v.active = false;
                }
                ui.small(if self.panel.vector.as_ref().is_some_and(|v| v.active) {
                    "Vector editor"
                } else {
                    "Raster editor"
                });
                if ui.button("Save project").clicked()
                    && let Err(e) = self.save_workspace()
                {
                    self.panel.failure = Some(e);
                }
                if ui.button("Scripts").clicked() {
                    self.panel.editor.visible = true;
                }
            });
            ui.horizontal(|ui| {
                if let Some(v) = &mut self.panel.vector
                    && v.active
                {
                    vectorcraft_ui_egui::menus::menu_bar(&mut v.app, ui);
                } else {
                    photocraft_ui_egui::menus::menu_bar(&mut self.photo, ui);
                }
            });
            let mut paths: Vec<_> = self
                .panel
                .bindings
                .iter()
                .filter(|(_, b)| {
                    self.photo
                        .session
                        .documents()
                        .iter()
                        .any(|d| d.doc.id == b.doc)
                })
                .map(|(p, _)| p.clone())
                .collect();
            if let Some(v) = &self.panel.vector {
                paths.extend(v.open_paths());
            }
            paths.sort();
            paths.dedup();
            let active = self
                .panel
                .vector
                .as_ref()
                .and_then(|v| {
                    if v.active {
                        v.active_path().map(Path::to_path_buf)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    self.photo.session.active().and_then(|d| {
                        self.panel
                            .bindings
                            .iter()
                            .find(|(_, b)| b.doc == d.doc.id)
                            .map(|(p, _)| p.clone())
                    })
                });
            egui::ScrollArea::horizontal()
                .id_salt("all-open-files")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for path in paths {
                            let label = format!(
                                "{} · {}",
                                path.file_name().unwrap_or_default().to_string_lossy(),
                                if is_svg(&path) { "SVG" } else { "Image" }
                            );
                            if ui
                                .selectable_label(active.as_ref() == Some(&path), label)
                                .on_hover_text(path.display().to_string())
                                .clicked()
                                && let Err(e) = self.panel.open_file(&mut self.photo, &path)
                            {
                                self.panel.failure = Some(e);
                            }
                        }
                    });
                });
        });
    }

    fn save_for_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        let result = self.panel.update_artwork(&mut self.photo).and_then(|()| {
            self.panel.vector_tick();
            self.save_workspace()
        });
        if let Err(e) = result {
            self.panel.failure = Some(e);
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return;
        }
        // Managed documents are saved in the SkinCraft project; neither engine should offer to
        // overwrite the original runtime assets. Independent, unbound documents keep their prompts.
        if let Some(v) = &mut self.panel.vector {
            let ids: Vec<_> = v.app.session.documents().iter().map(|d| d.uid).collect();
            for uid in ids {
                if let Some(d) = v.app.session.document_mut(uid) {
                    d.mark_saved();
                }
            }
        }
        let active = self.photo.session.active_index();
        let ids: Vec<_> = self
            .photo
            .session
            .documents()
            .iter()
            .map(|d| d.doc.id)
            .collect();
        for id in ids {
            if let Some(i) = self
                .photo
                .session
                .documents()
                .iter()
                .position(|d| d.doc.id == id)
            {
                self.photo.session.set_active(i);
                if let Some(d) = self.photo.session.active_mut() {
                    d.saved_revision = d.revision;
                }
            }
        }
        if let Some(i) = active {
            self.photo.session.set_active(i);
        }
    }
}

pub(super) fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

/// Both native themes replace egui's global font definitions. Keep their named families
/// available together, including after the other editor initializes or changes its theme.
/// egui activates replacements next pass, so defer layout until the aliases are available.
fn shared_fonts(ctx: &egui::Context) -> bool {
    use egui::FontFamily::{Monospace, Name, Proportional};
    let aliases = [
        ("ui", Proportional),
        ("ui-semibold", Name("semibold".into())),
        ("mono", Monospace),
        ("semibold", Name("ui-semibold".into())),
        ("medium", Name("ui-semibold".into())),
    ];
    let mut fonts = ctx.fonts(|f| f.definitions().clone());
    if aliases
        .iter()
        .all(|(name, _)| fonts.families.contains_key(&Name((*name).into())))
    {
        return true;
    }
    for (name, from) in aliases {
        let stack = fonts
            .families
            .get(&from)
            .or_else(|| fonts.families.get(&Proportional))
            .cloned()
            .unwrap_or_default();
        fonts.families.entry(Name(name.into())).or_insert(stack);
    }
    ctx.set_fonts(fonts);
    ctx.request_repaint();
    false
}

impl App for SkinCraft {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.capture_events(input);
        if self.panel.vector_active() {
            if let Some(v) = &mut self.panel.vector {
                v.app.raw_input_hook(input);
            }
        } else {
            self.photo.raw_input_hook(ctx, input);
        }
    }
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.native_tick(ctx);
        self.control_tick(ctx);
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.ask(native::Action::Open, ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
            && let Err(e) = self.save_workspace()
        {
            self.panel.failure = Some(e);
        }
        self.save_for_close(ctx);
        if let Some(v) = &mut self.panel.vector
            && v.active
        {
            v.app.logic(ctx);
        } else {
            self.photo.logic(ctx, frame);
        }
        self.fonts_ready = shared_fonts(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("SkinCraft".into()));
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !self.fonts_ready {
            return;
        }
        self.tabs(ui);
        self.panel.show(&mut self.photo, ui);
        if let Some(v) = &mut self.panel.vector
            && v.active
        {
            // The engines have independent widget namespaces, focus and document histories.
            v.app.ui.screen_mode = 2; // Host supplies menus/title; keep the native tools, panels and canvas.
            ui.push_id("skincraft-vector-editor", |ui| {
                vectorcraft_ui_egui::chrome::control_bar(&mut v.app, ui);
                if v.app.session.active().is_some() {
                    v.app.ui(ui);
                } else {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.label("Open an SVG from Noteskin Files.");
                    });
                }
            });
        } else {
            ui.push_id("skincraft-raster-editor", |ui| {
                if self.photo.session.active().is_some() {
                    self.photo.ui(ui, frame);
                } else {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.label("Open an image from Noteskin Files.");
                    });
                }
            });
        }
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title("SkinCraft".into()));
    }
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.panel.vector_tick();
        if let Err(e) = self
            .panel
            .update_artwork(&mut self.photo)
            .and_then(|()| self.save_workspace())
        {
            self.panel.failure = Some(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn danoni_proof_draws_in_skincraft() {
        let root = crate::project::tests::fixture("danoni-proof");
        let folder = ddi_desktop::images::assets()
            .join("noteskins/dance/DanOni")
            .canonicalize()
            .unwrap();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1600.0, 1000.0))
            .with_pixels_per_point(1.0)
            .wgpu()
            .build_eframe(|cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = SkinCraft::from_render_state(
                    cc.wgpu_render_state.clone().unwrap(),
                    root.join("profile"),
                )
                .unwrap();
                app.panel.folder = folder.to_string_lossy().into_owned();
                app.panel.load().unwrap();
                app.panel.running = false;
                app.panel.time = 3.8;
                app.panel
                    .open_file(&mut app.photo, &folder.join("_onigiri shape.svg"))
                    .unwrap();
                app
            });
        h.run_steps(6);
        assert!(
            h.state().panel.failure.is_none(),
            "{:?}",
            h.state().panel.failure
        );
        assert!(
            h.state()
                .panel
                .project
                .as_ref()
                .unwrap()
                .coverage
                .iter()
                .any(|s| s.contains("Onigiri"))
        );
        let shot =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/danoni-proof.png");
        h.render().unwrap().save(shot).unwrap();
        let pixels = super::super::tests::field_pixels(&h.state().panel);
        let blue = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[2] > 210 && (80..125).contains(&p[0]) && p[0].abs_diff(p[1]) < 3)
            .count();
        assert!(
            blue > 30,
            "tap tint must survive field-owned root colors: {blue} blue pixels"
        );
        let frames = (h.state().panel.duration * 60.0).ceil() as u32;
        for id in ["itg", "danoni"] {
            h.state_mut().panel.options.rules = super::super::preview_rules(id).unwrap();
            for auto in [false, true] {
                h.state_mut().panel.options.autoplay_flashes = auto;
                for time in (0..=frames).map(|i| f64::from(i) / 60.0) {
                    h.state_mut().panel.time = time;
                    h.run_steps(1);
                    assert!(
                        h.state().panel.failure.is_none(),
                        "at {time}, auto {auto}: {:?}",
                        h.state().panel.failure
                    );
                }
            }
        }
        drop(h);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dito_vector_edit_reaches_playfield_and_survives_project_restore() {
        let root = crate::project::tests::fixture("skincraft");
        let layout = Layout::builtin("danoni-7").unwrap();
        let original = ddi_desktop::images::assets().join("noteskins/dance/Dito");
        Project::open(&original, &layout)
            .unwrap()
            .export(&root.join("copy"), &HashMap::new())
            .unwrap();
        let folder = root.join("copy/NoteSkins/dance/Dito");
        let source = folder.join("_down receptor.svg");
        let original_bytes = std::fs::read(&source).unwrap();
        let bitmap = folder.join("unused.png");
        std::fs::copy(root.join("dance/Test/Down Tap Note.png"), &bitmap).unwrap();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1600.0, 1000.0))
            .with_pixels_per_point(1.0)
            .wgpu()
            .build_eframe(|cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = SkinCraft::from_render_state(
                    cc.wgpu_render_state.clone().unwrap(),
                    root.join("profile"),
                )
                .unwrap();
                app.panel.folder = folder.to_string_lossy().into_owned();
                app.panel.load().unwrap();
                app.panel.running = false;
                app.panel.time = 1.0;
                app.panel.open_file(&mut app.photo, &source).unwrap();
                app
            });
        h.run_steps(5);
        let shot_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/skin-studio");
        std::fs::create_dir_all(&shot_dir).unwrap();
        let v = &h.state().panel.vector.as_ref().unwrap().app;
        let center = v.canvas_rect.unwrap().center();
        let radius = v.view().unwrap().zoom as f32 * 32.0 * 0.8;
        let canvas = egui::Rect::from_center_size(center, egui::Vec2::splat(radius * 2.0));
        let capture = h.render().unwrap();
        let (mut ink, mut backdrop) = (0, 0);
        for (x, y, p) in capture.enumerate_pixels() {
            if canvas.contains(egui::pos2(x as f32, y as f32)) {
                ink += usize::from(p[0] > 245 && p[1] > 245 && p[2] > 245);
                backdrop += usize::from(
                    (70..160).contains(&p[0]) && p[0].abs_diff(p[1]) < 2 && p[1].abs_diff(p[2]) < 2,
                );
            }
        }
        assert!(
            ink > 1000 && backdrop > 1000,
            "white vector art must be visible against transparency"
        );
        capture
            .save(shot_dir.join("skincraft-initial.png"))
            .unwrap();
        let before = super::super::tests::field_pixels(&h.state().panel);
        let id = {
            let v = h.state_mut().panel.vector.as_mut().unwrap();
            let mut id = None;
            v.app.session.active().unwrap().doc.walk(|n| {
                if id.is_none() && matches!(n.kind, vectorcraft_doc::NodeKind::Path { .. }) {
                    id = Some(n.id.0);
                }
            });
            let id = id.unwrap();
            v.app.session.begin_interaction("Drag anchor").unwrap();
            v.app
                .session
                .preview(
                    "path.moveAnchor",
                    &serde_json::json!({
                        "id":id,"anchor":0,"x":3.0,"y":22.0
                    }),
                )
                .unwrap();
            id
        };
        h.run_steps(3);
        let after = super::super::tests::field_pixels(&h.state().panel);
        assert!(
            before != after,
            "an unfinished vector drag updates the real playfield"
        );
        assert!(
            !h.state().panel.edits.contains_key(&source),
            "SVG stays vector"
        );
        assert!(!h.state().panel.has_pending_sources());
        h.state_mut()
            .panel
            .vector
            .as_mut()
            .unwrap()
            .app
            .session
            .commit_interaction()
            .unwrap();
        h.run_steps(2);
        let accepted = h.state().panel.editor.edits[&source].clone();
        {
            let app = h.state_mut();
            app.panel.open_file(&mut app.photo, &bitmap).unwrap();
            assert!(!app.panel.vector_active());
        }
        h.run_steps(4);
        {
            let app = h.state_mut();
            app.panel.open_file(&mut app.photo, &source).unwrap();
            assert!(app.panel.vector_active());
            app.panel.save_project(&app.photo).unwrap();
        }
        h.run_steps(4);
        // Unsupported effects keep their native source but never replace the last valid game art.
        h.state_mut()
            .panel
            .vector
            .as_mut()
            .unwrap()
            .app
            .session
            .execute(
                "effect.apply",
                &serde_json::json!({"effect":"blur.gaussian","params":{"radius":4},"ids":[id]}),
            )
            .unwrap();
        h.run_steps(2);
        assert!(h.state().panel.has_pending_sources());
        assert_eq!(h.state().panel.editor.edits[&source], accepted);
        h.state_mut()
            .panel
            .vector
            .as_mut()
            .unwrap()
            .app
            .session
            .execute("edit.undo", &serde_json::json!({}))
            .unwrap();
        h.run_steps(2);
        assert!(!h.state().panel.has_pending_sources());
        {
            let app = h.state_mut();
            app.panel
                .project
                .as_ref()
                .unwrap()
                .export_texts(
                    &root.join("export"),
                    &app.panel.edits,
                    &app.panel.editor.edits,
                )
                .unwrap();
            let exported = root.join("export/NoteSkins/dance/Dito/_down receptor.svg");
            assert_eq!(std::fs::read_to_string(&exported).unwrap(), accepted);
            assert_eq!(std::fs::read(&source).unwrap(), original_bytes);
            let v = app.panel.vector.as_mut().unwrap();
            let i = v.app.session.active_index().unwrap();
            v.app.session.close_document(i);
            v.open(&source, Some(&accepted)).unwrap();
            v.app
                .session
                .execute("edit.undo", &serde_json::json!({}))
                .unwrap();
            app.panel.vector_tick();
            assert_ne!(
                app.panel.editor.edits[&source], accepted,
                "session history survives tab closure"
            );
            app.panel
                .vector
                .as_mut()
                .unwrap()
                .app
                .session
                .execute("edit.redo", &serde_json::json!({}))
                .unwrap();
            app.panel.vector_tick();
            assert_eq!(app.panel.editor.edits[&source], accepted);
            app.panel.save_project(&app.photo).unwrap();
        }
        h.run_steps(3);
        let native = h
            .state()
            .panel
            .vector
            .as_ref()
            .unwrap()
            .app
            .session
            .active()
            .unwrap();
        assert!(native.transparency_grid);
        assert_eq!(
            native.doc.setup.background,
            vectorcraft_doc::Background::Transparent
        );
        let shot = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/skin-studio/skincraft.png");
        h.render().unwrap().save(&shot).unwrap();
        assert!(
            h.state().panel.failure.is_none(),
            "{:?}",
            h.state().panel.failure
        );
        let rs = h.state().panel.rs.clone();
        let mut restored = SkinCraft::from_render_state(rs, root.join("profile")).unwrap();
        restored.panel.vector_tick();
        assert_eq!(restored.panel.editor.edits[&source], accepted);
        assert!(
            restored
                .panel
                .vector
                .as_ref()
                .unwrap()
                .retained
                .contains_key(&source)
        );
        assert!(restored.panel.drawings.contains_key(&bitmap));
        drop(restored);
        drop(h);
        std::fs::remove_dir_all(root).unwrap();
    }
}

impl SkinCraft {
    pub(super) fn save_workspace(&mut self) -> Result<(), String> {
        self.panel.update_artwork(&mut self.photo)?;
        self.panel.save_project(&self.photo)?;
        references::save(self)?;
        self.save_editor_preferences()
    }

    fn ask(&mut self, action: native::Action, ctx: &egui::Context) {
        if self.photo.file_dialog_open()
            || self
                .panel
                .vector
                .as_ref()
                .is_some_and(|v| v.app.file_dialog_open())
        {
            return;
        }
        if let Some(native) = &mut self.native {
            native.ask(action, ctx, self.panel.editor.active.as_deref());
        }
    }

    fn native_tick(&mut self, ctx: &egui::Context) {
        if let Some(mut native) = self.native.take() {
            native.sync(self);
            while let Some((action, paths)) = native.poll() {
                let result = (|| {
                    match action {
                        native::Action::Open => {
                            for path in paths {
                                self.open_any(Path::new(&path))?;
                            }
                        }
                        native::Action::Skin => {
                            if let Some(path) = paths.first() {
                                self.load_skin(Path::new(path))?;
                            }
                        }
                        native::Action::Export => {
                            if let Some(path) = paths.first() {
                                let name = self
                                    .panel
                                    .project
                                    .as_ref()
                                    .and_then(|p| p.dir.file_name())
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                let tick = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map_err(|e| e.to_string())?
                                    .as_millis();
                                let target =
                                    Path::new(path).join(format!("{name}-SkinCraft-{tick}"));
                                self.export_skin(&target)?;
                                self.panel.output = target.to_string_lossy().into_owned();
                            }
                        }
                        native::Action::TextCopy => {
                            if let Some(path) = paths.first() {
                                let source = self
                                    .panel
                                    .editor
                                    .active
                                    .as_ref()
                                    .ok_or("Open a text file first")?;
                                let text = self
                                    .panel
                                    .editor
                                    .edits
                                    .get(source)
                                    .cloned()
                                    .map(Ok)
                                    .unwrap_or_else(|| files::read_text(source))?;
                                native::write(path, text.as_bytes(), &native.protected)?;
                            }
                        }
                    }
                    Ok::<_, String>(())
                })();
                if let Err(e) = result {
                    self.panel.failure = Some(e);
                }
            }
            native.sync(self);
            self.native = Some(native);
        }
        if self.references_at.elapsed().as_secs_f32() >= 1.0 {
            if let Err(e) = references::save(self) {
                self.panel.failure = Some(e);
            }
            self.references_at = Instant::now();
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    pub(super) fn load_skin(&mut self, path: &Path) -> Result<(), String> {
        self.save_workspace()?;
        self.panel.folder = path.to_string_lossy().into_owned();
        self.panel.load()?;
        Ok(())
    }

    pub(super) fn open_any(&mut self, path: &Path) -> Result<(), String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if path.is_dir()
            && (path.join("metrics.ini").is_file() || path.join("NoteSkin.lua").is_file())
        {
            return self.load_skin(&path);
        }
        if let Some(file) = self.panel.files.iter().find(|f| f.path == path).cloned() {
            if file.image {
                return self.panel.open_file(&mut self.photo, &path);
            }
            if file.text {
                return self.panel.editor.open(&path);
            }
        }
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        if matches!(
            ext.as_str(),
            "svg" | "vectorcraft" | "drawcraft" | "ai" | "eps" | "pdf"
        ) {
            let v = self.panel.vector.as_mut().ok_or("No vector workspace")?;
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            vectorcraft_ui_egui::io::open_bytes(
                &mut v.app,
                &path.to_string_lossy(),
                &bytes,
                Some(path.to_string_lossy().into_owned()),
            )?;
            v.active = true;
        } else if files::image(&path)
            || matches!(ext.as_str(), "pcraft" | "psd" | "psb" | "ora" | "xcf")
        {
            let source = path.to_string_lossy().into_owned();
            if let Some(index) = self
                .photo
                .session
                .documents()
                .iter()
                .position(|d| d.path.as_ref() == Some(&source))
            {
                self.photo.session.set_active(index);
            } else {
                let opened =
                    photocraft_automation::files::open(&path).map_err(|e| e.to_string())?;
                if opened.document.walk().iter().any(|(_, _, layer)| {
                    matches!(layer.content, photocraft_doc::LayerContent::Text(_))
                }) {
                    // Keep the native missing-font and profile dialogs for typed documents.
                    self.photo.open_path(&source)?;
                } else {
                    // No text means no font validation is needed. Native File Open otherwise
                    // initializes the full system-font catalog even for a small plain PNG.
                    self.photo
                        .session
                        .add_document(opened.document, Some(source));
                    if let Some(doc) = self.photo.session.active_mut() {
                        doc.source_read_only = opened.source_read_only;
                    }
                    if !opened.warnings.is_empty() {
                        self.panel.message = opened.warnings.join("\n");
                    }
                }
            }
            if let Some(v) = &mut self.panel.vector {
                v.active = false;
            }
        } else {
            return Err(
                "Open this file's noteskin folder first to edit scripts or text in the project"
                    .into(),
            );
        }
        self.panel.editor.visible = false;
        self.panel.message="Reference document opened. Save As keeps an independent editable file; the noteskin preview follows project files.".into();
        Ok(())
    }

    pub(super) fn export_skin(&mut self, path: &Path) -> Result<(), String> {
        self.panel.vector_tick();
        self.panel.update_artwork(&mut self.photo)?;
        if self.panel.has_pending_sources() {
            return Err("Fix source errors and wait for pending reloads before export".into());
        }
        self.panel
            .project
            .as_ref()
            .ok_or("No noteskin")?
            .export_texts(path, &self.panel.edits, &self.panel.editor.edits)?;
        self.panel.message = format!("Exported skin to {}", path.display());
        Ok(())
    }

    pub(super) fn save_copy(&mut self, path: &Path) -> Result<(), String> {
        let bytes = if self.panel.vector_active() {
            let doc = &self
                .panel
                .vector
                .as_ref()
                .ok_or("No vector editor")?
                .app
                .session
                .active()
                .ok_or("No vector document")?
                .doc;
            if is_svg(path) {
                vector::runtime_svg(doc)?.0.into_bytes()
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("vectorcraft"))
            {
                vectorcraft_format::save_with(doc, &Default::default())
                    .map_err(|e| e.to_string())?
            } else {
                return Err("Use .svg or .vectorcraft for a vector copy".into());
            }
        } else {
            let doc = &self.photo.session.active().ok_or("No raster document")?.doc;
            photocraft_automation::files::save_bytes(
                doc,
                &path.to_string_lossy(),
                None,
                &Default::default(),
            )
            .map_err(|e| e.to_string())?
            .0
        };
        self.check_write(path)?;
        workspace::atomic(path, &bytes)
    }
}

impl SkinCraft {
    fn save_editor_preferences(&self) -> Result<(), String> {
        if let Some(home) = &self.panel.home
            && let Some(v) = &self.panel.vector
        {
            let bytes =
                serde_json::to_vec(&serde_json::json!({"prefs":v.app.session.prefs,"ui":v.app.ui}))
                    .map_err(|e| e.to_string())?;
            workspace::atomic(&home.join("vectorcraft-prefs.json"), &bytes)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use serde_json::{Value, json};
    use std::io::{BufRead, Write};
    use std::sync::Arc;

    fn harness(home: &Path) -> egui_kittest::Harness<'static, SkinCraft> {
        let home = home.to_path_buf();
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(1700.0, 1050.0))
            .with_pixels_per_point(1.0)
            .wgpu()
            .build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = SkinCraft::from_render_state(
                    cc.wgpu_render_state.clone().unwrap(),
                    home.clone(),
                )
                .unwrap();
                app.native = Some(native::Runtime::install(&mut app, cc).unwrap());
                app.control = Some(control::Server::start(&home, cc.egui_ctx.clone()).unwrap());
                app.panel.running = false;
                app
            })
    }

    #[test]
    fn native_save_dialogs_cancel_write_and_preserve_reference_documents() {
        let root = crate::project::tests::fixture("native-services");
        let home = root.join("profile");
        let mut h = harness(&home);
        h.run_steps(6);
        let vector_file = home.join("native.vectorcraft");
        {
            let app = h.state_mut();
            let target = vector_file.to_string_lossy().into_owned();
            app.panel.vector.as_mut().unwrap().app.services.start_pick =
                Some(Box::new(move |_| {
                    let (tx, rx) = std::sync::mpsc::channel();
                    tx.send(vec![target.clone()]).unwrap();
                    Some(rx)
                }));
            let pending = app
                .panel
                .vector
                .as_mut()
                .unwrap()
                .app
                .run("file.saveAs", json!({"options":{}}));
            assert!(pending.is_ok() || pending.as_ref().err().is_some_and(|e| e == "cancelled"));
            assert!(app.panel.vector.as_ref().unwrap().app.file_dialog_open());
        }
        for _ in 0..80 {
            h.run_steps(1);
            if vector_file.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            vector_file.is_file(),
            "native vector Save As writes the selected path"
        );
        vectorcraft_format::load(&std::fs::read(&vector_file).unwrap()).unwrap();
        let raster_file = home.join("native.pcraft");
        {
            let app = h.state_mut();
            app.open_any(&root.join("dance/Test/Down Tap Note.png"))
                .unwrap();
            let target = raster_file.to_string_lossy().into_owned();
            app.photo.services.file_dialog = Some(Box::new(move |request, _, reply| {
                assert!(matches!(
                    request,
                    photocraft_ui_egui::FileDialogRequest::Save { .. }
                ));
                reply.send(Some(photocraft_ui_egui::FileDialogAnswer::SaveTo(
                    target.clone(),
                )));
            }));
            app.photo.save_as(None).unwrap();
        }
        h.run_steps(5);
        assert!(
            raster_file.is_file(),
            "native raster Save As writes the selected path"
        );
        photocraft_format::load_from_bytes(&std::fs::read(&raster_file).unwrap()).unwrap();
        {
            let app = h.state_mut();
            app.photo.services.file_dialog = Some(Box::new(|_, _, reply| reply.send(None)));
            app.photo.save_as(None).unwrap();
        }
        h.run_steps(3);
        assert!(!h.state().photo.file_dialog_open());
        let source = h
            .state()
            .panel
            .files
            .iter()
            .find(|f| is_svg(&f.path))
            .unwrap()
            .path
            .clone();
        let original = std::fs::read(&source).unwrap();
        assert!(
            h.state_mut().photo.services.write.as_mut().unwrap()(
                &source.to_string_lossy(),
                b"refuse"
            )
            .is_err()
        );
        assert_eq!(std::fs::read(source).unwrap(), original);
        {
            let app = h.state_mut();
            app.photo
                .session
                .execute("image.adjustments.invert", json!({}))
                .unwrap();
            app.save_workspace().unwrap();
        }
        let refs = std::fs::read(home.join("references/session.json")).unwrap();
        let record: Value = serde_json::from_slice(&refs).unwrap();
        assert!(
            record["records"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["vector"] == false)
        );
        let rs = h.state().panel.rs.clone();
        let restored = SkinCraft::from_render_state(rs, home.clone()).unwrap();
        assert!(
            restored
                .photo
                .session
                .documents()
                .iter()
                .any(|d| d.path.as_deref() == raster_file.to_str()),
            "independent edited files survive window/project restore"
        );
        drop(restored);
        drop(h);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mcp_stdio_drives_the_same_live_editors_and_captures_the_window() {
        let root = crate::project::tests::fixture("mcp-live");
        let home = root.join("profile");
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let binary = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/photocraft/{profile}/skincraft"));
        assert!(
            binary.is_file(),
            "build --features skincraft --bin skincraft before this integration test"
        );
        let mut h = harness(&home);
        h.run_steps(6);
        let svg = h
            .state()
            .panel
            .files
            .iter()
            .find(|f| f.name == "_down receptor.svg")
            .unwrap()
            .path
            .clone();
        let image = home.join("reference.png");
        std::fs::copy(root.join("dance/Test/Down Tap Note.png"), &image).unwrap();
        let home_client = home.clone();
        let client = std::thread::spawn(move || {
            let mut child = std::process::Command::new(binary)
                .arg("mcp")
                .env("DDI_STUDIO_HOME", &home_client)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::inherit())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
            let mut sequence = 0;
            let mut rpc = |method: &str, params: Value| -> Value {
                sequence += 1;
                writeln!(
                    input,
                    "{}",
                    json!({"jsonrpc":"2.0","id":sequence,"method":method,"params":params})
                )
                .unwrap();
                input.flush().unwrap();
                loop {
                    let mut line = String::new();
                    assert!(
                        output.read_line(&mut line).unwrap() > 0,
                        "MCP closed unexpectedly"
                    );
                    let v: Value = serde_json::from_str(&line).unwrap();
                    if v["id"] == sequence {
                        if method == "initialize" {
                            writeln!(
                                input,
                                "{}",
                                json!({"jsonrpc":"2.0","method":"notifications/initialized"})
                            )
                            .unwrap();
                            input.flush().unwrap();
                        }
                        return v;
                    }
                }
            };
            let initialized = rpc(
                "initialize",
                json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"SkinCraft-test","version":"1"}}),
            );
            assert_eq!(initialized["result"]["serverInfo"]["name"], "skincraft");
            let tools = rpc("tools/list", json!({}));
            assert!(tools["result"]["tools"].as_array().unwrap().len() >= 13);
            let mut call = |name: &str, args: Value| -> Value {
                eprintln!("MCP smoke: {name}");
                let v = rpc("tools/call", json!({"name":name,"arguments":args}));
                assert!(v.get("error").is_none(), "{name}: {v}");
                assert_ne!(v["result"]["isError"], true, "{name}: {v}");
                v["result"].clone()
            };
            call("open_file", json!({"path":svg}));
            let preview = call(
                "set_preview",
                json!({"running":false,"autoplay_flashes":true,"ruleset":"danoni"}),
            );
            let preview: Value =
                serde_json::from_str(preview["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(preview["autoplayFlashes"], true);
            assert_eq!(preview["ruleset"], "danoni");
            call(
                "set_preview",
                json!({"autoplay_flashes":false,"ruleset":"itg"}),
            );
            let commands = call(
                "editor_request",
                json!({"editor":"vector","method":"engine.commands"}),
            );
            assert!(commands.to_string().contains("path.moveAnchor"));
            call("save_file", json!({"path":home_client.join("copy.svg")}));
            call("list_files", json!({}));
            let script = svg.parent().unwrap().join("NoteSkin.lua");
            let source = call("read_text", json!({"path":script}));
            let content: Value =
                serde_json::from_str(source["content"][0]["text"].as_str().unwrap()).unwrap();
            call(
                "write_text",
                json!({"path":script,"text":format!("{}\n-- SkinCraft MCP check\n",content["text"].as_str().unwrap())}),
            );
            call("export_skin", json!({"path":home_client.join("exported")}));
            call("save_project", json!({}));
            // Independent artwork uses PhotoCraft and stays separate from noteskin overrides.
            call("open_file", json!({"path":image}));
            call(
                "editor_request",
                json!({"editor":"raster","method":"engine.execute","params":{"command":"image.adjustments.invert","params":{}}}),
            );
            call("save_file", json!({"path":home_client.join("copy.pcraft")}));
            let capture = call(
                "screenshot",
                json!({"path":home_client.join("mcp-window.png")}),
            );
            assert!(
                capture["content"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["type"] == "image")
            );
            call("save_project", json!({}));
            drop(call);
            drop(rpc);
            drop(input);
            let _ = child.kill();
            child.wait().unwrap();
        });
        let deadline = Instant::now() + std::time::Duration::from_secs(90);
        while !client.is_finished() && Instant::now() < deadline {
            h.run_steps(1);
            // kittest has no OS event loop: drain GPU submissions and deliver its rendered
            // image through the same raw-input hook that receives an eframe window capture.
            let image = h.render().unwrap();
            if let Some(p) = h.state().captures.first() {
                let data = egui::UserData::new(("skincraft".to_string(), p.token));
                let screenshot = egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                );
                let mut input = egui::RawInput::default();
                input.events.push(egui::Event::Screenshot {
                    viewport_id: egui::ViewportId::ROOT,
                    user_data: data,
                    image: Arc::new(screenshot),
                });
                let ctx = h.ctx.clone();
                h.state_mut().raw_input_hook(&ctx, &mut input);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(client.is_finished(), "MCP integration timed out");
        client.join().unwrap();
        assert!(home.join("copy.svg").is_file());
        assert!(home.join("copy.pcraft").is_file());
        assert!(home.join("mcp-window.png").is_file());
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/skin-studio/skincraft-mcp.png");
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::copy(home.join("mcp-window.png"), output).unwrap();
        drop(h);
        std::fs::remove_dir_all(root).unwrap();
    }
}
