use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ddi_chart::Layout;
use ddi_desktop::play::view::{Rgba, write_rgba};
use ddi_noteskin::build::TextureKey;
use ddi_render::field_preview::{FieldPreview, FieldPreviewOptions};
use eframe::egui_wgpu::RenderState;
use eframe::wgpu;
use photocraft_doc::DocId;
use photocraft_ui_egui::{PanelExtension, PhotocraftApp};

use crate::project::{self, Asset, Project};
use crate::{files, text, workspace};
#[path = "control.rs"]
#[cfg(feature = "skincraft")]
mod control;
#[cfg(feature = "skincraft")]
#[path = "host.rs"]
mod host;
#[cfg(feature = "skincraft")]
#[path = "native.rs"]
mod native;
#[path = "references.rs"]
#[cfg(feature = "skincraft")]
mod references;
#[cfg(feature = "skincraft")]
#[path = "vector.rs"]
mod vector;
#[path = "workspace_ui.rs"]
mod workspace_ui;
#[cfg(feature = "skincraft")]
pub use host::SkinCraft;

const FONT: &[u8] = include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");

pub(super) fn preview_rules(id: &str) -> Option<ddi_engine::rules::RulesetKey> {
    match id {
        "danoni" => Some(ddi_engine::rules::presets::danoni().key()),
        "itg" | "sm5" | "ddr-a" => ddi_engine::rules::presets::by_id(id).map(|r| r.key()),
        _ => None,
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedProject {
    folder: String,
    layout: String,
    bindings: Vec<(PathBuf, PathBuf)>,
}

struct Binding {
    doc: DocId,
    seen: Option<(u64, u64)>,
}

/// Embedded drawing feedback, with no game window or audio backend started.
pub struct Panel {
    rs: RenderState,
    preview: FieldPreview,
    project: Option<Project>,
    assets: Vec<Asset>,
    choices: Vec<PathBuf>,
    folder: String,
    selected: usize,
    layout: String,
    options: FieldPreviewOptions,
    bindings: HashMap<PathBuf, Binding>,
    edits: HashMap<PathBuf, Rgba>,
    output: String,
    message: String,
    failure: Option<String>,
    registered: Option<(u64, egui::TextureId)>,
    running: bool,
    time: f64,
    last: Instant,
    updates: u64,
    update_ms: f64,
    loaded: bool,
    collapsed: bool,
    duration: f64,
    home: Option<PathBuf>,
    saved: Vec<u8>,
    restore: Vec<(PathBuf, PathBuf)>,
    restoring: bool,
    files: Vec<files::File>,
    search: String,
    files_collapsed: bool,
    selected_file: Option<PathBuf>,
    thumbnails: HashMap<PathBuf, egui::TextureHandle>,
    thumbnail_failed: HashSet<PathBuf>,
    editor: text::Editor,
    applied_texts: HashMap<PathBuf, String>,
    rollback_texts: Option<HashMap<PathBuf, String>>,
    rollback_problems: Vec<String>,
    drawings: HashMap<PathBuf, photocraft_engine::DocState>,
    sizes: HashMap<PathBuf, [u32; 2]>,
    workspace_dir: Option<PathBuf>,
    checkpoint_dirty: bool,
    checkpoint_at: Instant,
    pending_open: Vec<PathBuf>,
    text_reload: Option<Instant>,
    reloads: u64,
    inspect_lane: usize,
    inspect_part: String,
    #[cfg(feature = "skincraft")]
    vector: Option<vector::Workspace>,
}

impl Panel {
    pub fn new(rs: RenderState) -> Self {
        let choices = project::choices();
        let folder = std::env::var("DDI_STUDIO_SKIN").ok().unwrap_or_else(|| {
            ddi_desktop::images::assets()
                .join("noteskins/dance/Dito")
                .to_string_lossy()
                .into_owned()
        });
        let preview = FieldPreview::new(
            &rs.device,
            &rs.queue,
            Some(wgpu::TextureFormat::Rgba16Float),
            &[FONT],
        );
        let home = std::env::var_os("DDI_STUDIO_HOME").map(PathBuf::from);
        let output = std::env::var("DDI_STUDIO_OUTPUT").unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/scratch/skin-studio/export")
                .to_string_lossy()
                .into_owned()
        });
        let mut panel = Self {
            rs,
            preview,
            project: None,
            assets: Vec::new(),
            choices,
            folder,
            selected: 0,
            layout: "danoni-7".into(),
            options: FieldPreviewOptions::default(),
            bindings: HashMap::new(),
            edits: HashMap::new(),
            output,
            message:
                "Choose a texture and open it to draw. File → Save As keeps your layered source."
                    .into(),
            failure: None,
            registered: None,
            running: true,
            time: 0.0,
            last: Instant::now(),
            updates: 0,
            update_ms: 0.0,
            loaded: false,
            collapsed: false,
            duration: 32.0,
            home,
            saved: Vec::new(),
            restore: Vec::new(),
            restoring: false,
            files: Vec::new(),
            search: String::new(),
            files_collapsed: false,
            selected_file: None,
            thumbnails: HashMap::new(),
            thumbnail_failed: HashSet::new(),
            editor: Default::default(),
            applied_texts: HashMap::new(),
            rollback_texts: None,
            rollback_problems: Vec::new(),
            drawings: HashMap::new(),
            sizes: HashMap::new(),
            workspace_dir: None,
            checkpoint_dirty: false,
            checkpoint_at: Instant::now(),
            pending_open: Vec::new(),
            text_reload: None,
            reloads: 0,
            inspect_lane: 0,
            inspect_part: "Tap Note".into(),
            #[cfg(feature = "skincraft")]
            vector: None,
        };
        panel.restore_settings();
        panel
    }

    #[cfg(feature = "skincraft")]
    fn at_home(rs: RenderState, home: PathBuf) -> Self {
        let mut panel = Self::new(rs);
        panel.home = Some(home);
        panel.restore.clear();
        panel.saved.clear();
        panel.restore_settings();
        panel
    }

    fn restore_settings(&mut self) {
        if let Some(path) = self.home.as_ref().map(|p| p.join("project.json"))
            && let Ok(meta) = std::fs::metadata(&path)
            && meta.len() < 256 << 10
            && let Ok(bytes) = std::fs::read(path)
        {
            match serde_json::from_slice::<SavedProject>(&bytes) {
                Ok(state) if state.bindings.len() <= 128 => {
                    self.folder = state.folder;
                    self.layout = state.layout;
                    self.restore = state.bindings;
                    self.saved = bytes;
                }
                _ => self.failure = Some("Could not read the saved authoring project".into()),
            }
        }
    }

    fn load(&mut self) -> Result<(), String> {
        let layout = Layout::builtin(&self.layout).ok_or("Unknown layout")?;
        let original = Project::open(Path::new(&self.folder), &layout)?;
        let changed_project = self
            .project
            .as_ref()
            .is_none_or(|old| old.dir != original.dir);
        let (project, prepared) = if !changed_project && !self.editor.edits.is_empty() {
            original.prepare_texts(
                &layout,
                self.options.play.scroll.reverse,
                &self.editor.edits,
                &self.candidate_dir(),
            )?
        } else {
            (
                original.clone(),
                original.prepare(&layout, self.options.play.scroll.reverse)?,
            )
        };
        let files = files::inventory(&project)?;
        if changed_project {
            self.edits.clear();
            self.bindings.clear();
            self.drawings.clear();
            self.sizes.clear();
            self.editor = text::Editor::for_project(&original.dir);
            #[cfg(feature = "skincraft")]
            if self.project.is_some() && self.vector.is_some() {
                self.vector = Some(vector::Workspace::new());
            }

            self.applied_texts.clear();
            self.rollback_texts = None;
            self.thumbnails.clear();
            self.thumbnail_failed.clear();
            self.pending_open.clear();
            self.selected_file = None;
            if self.project.is_some() {
                self.restore.clear();
            }
        }
        let previous_problems = self
            .preview
            .noteskin()
            .map(|p| p.problems().to_vec())
            .unwrap_or_default();
        let mut assets = Vec::new();
        let device = &self.rs.device;
        let queue = &self.rs.queue;
        self.options.layout = self.layout.clone();
        self.preview.set_options(self.options.clone());
        self.duration =
            ddi_engine::demo::Demo::new(&self.layout, ddi_render::field_preview::BPM, 1.0)
                .map_or(32.0, |d| d.length());
        self.preview
            .set_noteskin(device, queue, Some(prepared.play), |renderer, play| {
                let keys = play.texture_keys().to_vec();
                for (i, key) in keys.iter().enumerate() {
                    let id = match key {
                        TextureKey::BuiltIn(name) => renderer.add_builtin_art(device, queue, name),
                        _ => {
                            let Some(image) = prepared.images.get(i).and_then(Option::as_ref)
                            else {
                                continue;
                            };
                            let mipmaps = play.texture_mipmaps(i);
                            let texture = renderer.create_noteskin_texture(
                                device,
                                image.size[0],
                                image.size[1],
                                mipmaps,
                            );
                            let source = project.source(key);
                            let shown = source
                                .as_ref()
                                .and_then(|s| self.edits.get(s))
                                .unwrap_or(image);
                            if shown.size == image.size {
                                write_rgba(queue, &texture, shown);
                            } else {
                                write_rgba(queue, &texture, image);
                            }
                            if mipmaps {
                                renderer.generate_noteskin_mipmaps(device, queue, &texture);
                            }
                            let id = renderer.add_noteskin_texture(device, &texture);
                            if let Some(source) = source {
                                let label = format!(
                                    "{}/{}",
                                    source
                                        .parent()
                                        .and_then(Path::file_name)
                                        .unwrap_or_default()
                                        .to_string_lossy(),
                                    source.file_name().unwrap_or_default().to_string_lossy()
                                );
                                assets.push(Asset {
                                    source,
                                    label,
                                    image: image.clone(),
                                    texture,
                                    mipmaps,
                                });
                            }
                            id
                        }
                    };
                    play.set_texture(i, id);
                }
            });
        self.assets = assets;
        self.selected = self.selected.min(self.assets.len().saturating_sub(1));
        self.files = files;
        self.thumbnails.clear();
        self.thumbnail_failed.clear();
        self.project = Some(project);
        if self.applied_texts != self.editor.edits {
            self.rollback_texts = Some(self.applied_texts.clone());
            self.rollback_problems = previous_problems;
        }
        self.applied_texts = self.editor.edits.clone();
        if changed_project {
            self.restore_workspace()?;
        }
        self.reloads += 1;
        self.loaded = true;
        self.failure = None;
        Ok(())
    }

    pub fn open_artwork(&mut self, app: &mut PhotocraftApp) -> Result<DocId, String> {
        let source = self
            .assets
            .get(self.selected)
            .ok_or("This skin has no editable image textures")?
            .source
            .clone();
        self.open_image(app, &source)
    }

    fn bind_active(&mut self, app: &PhotocraftApp) -> Result<(), String> {
        let asset = self.assets.get(self.selected).ok_or("Choose a texture")?;
        if self.editor.edits.contains_key(&asset.source) {
            return Err("This SVG has source edits. Edit it in the vector/source editor before binding raster artwork.".into());
        }
        let state = app
            .session
            .active()
            .ok_or("Open an artwork document first")?;
        if state.doc.size.width > 4096
            || state.doc.size.height > 4096
            || state.doc.size.area() > 4 << 20
        {
            return Err("Live artwork must fit within 4096 px per side and 4 megapixels".into());
        }
        let doc = state.doc.id;
        if self
            .bindings
            .iter()
            .any(|(source, b)| b.doc == doc && source != &asset.source)
        {
            return Err("This document is already bound to another texture".into());
        }
        self.sizes.insert(asset.source.clone(), asset.image.size);
        self.bindings
            .insert(asset.source.clone(), Binding { doc, seen: None });
        self.checkpoint_dirty = true;
        Ok(())
    }

    fn update_artwork(&mut self, app: &mut PhotocraftApp) -> Result<(), String> {
        let closed: Vec<_> = self
            .bindings
            .iter()
            .filter(|(_, b)| !app.session.documents().iter().any(|s| s.doc.id == b.doc))
            .map(|(p, _)| p.clone())
            .collect();
        if !closed.is_empty() {
            self.checkpoint_dirty = true;
        }
        for source in closed {
            self.bindings.remove(&source);
        }
        let started = Instant::now();
        for (source, binding) in &mut self.bindings {
            let Some(index) = app
                .session
                .documents()
                .iter()
                .position(|s| s.doc.id == binding.doc)
            else {
                continue;
            };
            let Some((doc, revision, preview)) = app.artwork_snapshot(index) else {
                continue;
            };
            if binding.seen == Some((revision, preview)) {
                continue;
            }
            let size = self
                .assets
                .iter()
                .find(|a| &a.source == source)
                .map(|a| a.image.size)
                .or_else(|| self.sizes.get(source).copied())
                .ok_or("Unknown artwork size")?;
            let image = composite(&doc, size)?;
            if let Some(state) = app.session.documents().get(index)
                && self
                    .drawings
                    .get(source)
                    .is_none_or(|old| old.revision != state.revision || old.doc.id != state.doc.id)
            {
                self.drawings.insert(source.clone(), state.clone());
                self.checkpoint_dirty = true;
            }
            // A source can be shared by more than one actor/texture entry.
            for a in self.assets.iter().filter(|a| &a.source == source) {
                write_rgba(&self.rs.queue, &a.texture, &image);
                if a.mipmaps {
                    self.preview.renderer_mut().generate_noteskin_mipmaps(
                        &self.rs.device,
                        &self.rs.queue,
                        &a.texture,
                    );
                }
            }
            self.thumbnails.remove(source);
            self.edits.insert(source.clone(), image);
            binding.seen = Some((revision, preview));
            self.updates += 1;
            self.update_ms = started.elapsed().as_secs_f64() * 1000.0;
        }
        Ok(())
    }

    fn controls(&mut self, app: &mut PhotocraftApp, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Noteskin Preview");
            if ui.small_button("Collapse").clicked() {
                self.collapsed = true;
            }
        });
        egui::ComboBox::from_id_salt("studio-skins")
            .selected_text("Bundled skins…")
            .show_ui(ui, |ui| {
                for path in self.choices.clone() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if ui.selectable_label(false, name).clicked() {
                        if let Err(e) = self.save_project(app) {
                            self.failure = Some(e);
                            return;
                        }
                        self.folder = path.to_string_lossy().into_owned();
                        self.loaded = false;
                    }
                }
            });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.folder)
                    .desired_width(ui.available_width() - 65.0),
            );
            if ui.button("Load").clicked() {
                match self.save_project(app) {
                    Ok(()) => self.loaded = false,
                    Err(e) => self.failure = Some(e),
                }
            }
        });
        let before_layout = self.layout.clone();
        egui::ComboBox::from_label("Layout")
            .selected_text(&self.layout)
            .show_ui(ui, |ui| {
                for id in [
                    "dance-single",
                    "dance-solo",
                    "dance-double",
                    "danoni-5",
                    "danoni-7",
                    "danoni-9A",
                    "danoni-9B",
                    "pump-single",
                    "pump-double",
                ] {
                    if Layout::builtin(id).is_some() {
                        ui.selectable_value(&mut self.layout, id.to_string(), id);
                    }
                }
            });
        if before_layout != self.layout {
            self.checkpoint_dirty = true;
            self.loaded = false;
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.running, "Play");
            if ui.button("Restart").clicked() {
                self.time = 0.0;
            }
            let mut reverse = self.options.play.scroll.reverse;
            if ui.checkbox(&mut reverse, "Reverse").changed() {
                self.options.play.scroll.reverse = reverse;
                self.loaded = false;
            }
        });
        let mut position = self.time.rem_euclid(self.duration);
        ui.checkbox(&mut self.options.autoplay_flashes, "Autoplay flashes");
        egui::ComboBox::from_label("Rules")
            .selected_text(&self.options.rules.id)
            .show_ui(ui, |ui| {
                for (id, label) in [
                    ("itg", "ITG"),
                    ("danoni", "DanOni"),
                    ("sm5", "StepMania 5"),
                    ("ddr-a", "DDR A"),
                ] {
                    if let Some(rules) = preview_rules(id) {
                        ui.selectable_value(&mut self.options.rules, rules, label);
                    }
                }
            });
        if ui
            .add(egui::Slider::new(&mut position, 0.0..=self.duration).text("Loop time (s)"))
            .changed()
        {
            self.time = position;
        }
        ui.add(egui::Slider::new(&mut self.options.background, 0.0..=1.0).text("Backdrop"));
        let mut speed = match self.options.play.scroll.speed {
            ddi_engine::scroll::SpeedMod::XMod(v) => v,
            _ => 1.0,
        };
        if ui
            .add(egui::Slider::new(&mut speed, 0.5..=4.0).text("Scroll speed"))
            .changed()
        {
            self.options.play.scroll.speed = ddi_engine::scroll::SpeedMod::XMod(speed);
        }
        let label = self
            .assets
            .get(self.selected)
            .map(|a| a.label.as_str())
            .unwrap_or("No texture");
        egui::ComboBox::from_label("Artwork")
            .selected_text(label)
            .width(ui.available_width() - 65.0)
            .truncate()
            .show_ui(ui, |ui| {
                for (i, asset) in self.assets.iter().enumerate() {
                    ui.selectable_value(&mut self.selected, i, &asset.label);
                }
            });
        ui.horizontal(|ui| {
            if ui.button("Open to draw").clicked()
                && let Some(source) = self.assets.get(self.selected).map(|a| a.source.clone())
                && let Err(e) = self.open_file(app, &source)
            {
                self.failure = Some(e);
            }
            if ui
                .add_enabled(
                    !self.vector_active(),
                    egui::Button::new("Use active document"),
                )
                .clicked()
                && let Err(e) = self.bind_active(app)
            {
                self.failure = Some(e);
            }
        });
        if ui
            .button("New onigiri working copy")
            .on_hover_text(
                "Create a new skin with editable onigiri notes and receptor in the export folder",
            )
            .clicked()
        {
            let result = self
                .project
                .as_ref()
                .ok_or_else(|| "Load a skin first".to_string())
                .and_then(|p| {
                    if self.has_pending_sources() { return Err("Wait for pending reloads and fix source errors before creating a child skin".into()); }
                    p.add_onigiri_texts(Path::new(&self.output), &self.edits, &self.editor.edits)
                });
            match result {
                Ok(path) => {
                    if let Err(e) = self.save_project(app) {
                        self.failure = Some(e);
                        return;
                    }
                    self.folder = path.to_string_lossy().into_owned();
                    self.loaded = false;
                    self.message = "Created dedicated onigiri art. Choose its _studio onigiri tap or receptor texture and Open to draw.".into();
                    self.failure = None;
                }
                Err(e) => self.failure = Some(e),
            }
        }
        if let Some(asset) = self.assets.get(self.selected) {
            ui.small(format!(
                "{} × {} px · {}",
                asset.image.size[0],
                asset.image.size[1],
                if self.bindings.contains_key(&asset.source) {
                    "LIVE"
                } else if self.editor.edits.contains_key(&asset.source) {
                    "SVG source"
                } else {
                    "original"
                }
            ));
        }
        if let Some(error) = &self.failure {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.small(&self.message);
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width().clamp(100.0, 700.0);
        let height = (ui.available_height() - 155.0).clamp(210.0, 600.0);
        let ppp = ui.ctx().pixels_per_point().min(2.0);
        self.preview.set_options(self.options.clone());
        self.preview.seek(self.time);
        let mut encoder = self
            .rs
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("studio-preview"),
            });
        self.preview.draw(
            &self.rs.device,
            &self.rs.queue,
            &mut encoder,
            [(width * ppp) as u32, (height * ppp) as u32],
            0.0,
        );
        self.rs.queue.submit([encoder.finish()]);
        if self
            .registered
            .is_none_or(|(g, _)| g != self.preview.generation())
            && let Some(texture) = self.preview.texture()
        {
            let view = texture.create_view(&Default::default());
            let mut renderer = self.rs.renderer.write();
            let id = match self.registered {
                Some((_, id)) => {
                    renderer.update_egui_texture_from_wgpu_texture(
                        &self.rs.device,
                        &view,
                        wgpu::FilterMode::Linear,
                        id,
                    );
                    id
                }
                None => renderer.register_native_texture(
                    &self.rs.device,
                    &view,
                    wgpu::FilterMode::Linear,
                ),
            };
            self.registered = Some((self.preview.generation(), id));
        }
        if let Some((_, id)) = self.registered {
            ui.image((id, egui::vec2(width, height)));
        }
        if let Some(error) = self.preview.noteskin().and_then(|s| {
            s.lua_failure().map(str::to_owned).or_else(|| {
                if self.rollback_texts.is_some() {
                    s.problems()
                        .iter()
                        .find(|p| !self.rollback_problems.contains(p))
                        .cloned()
                } else {
                    None
                }
            })
        }) {
            if let Some(previous) = self.rollback_texts.take() {
                let drafts = std::mem::replace(&mut self.editor.edits, previous);
                let restored = self.load();
                self.editor.edits = drafts;
                self.rollback_texts = None;
                self.failure = Some(match restored {
                    Ok(()) => format!("{error} (last working preview restored)"),
                    Err(e) => format!("{error}; could not restore preview: {e}"),
                });
            }
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Script error: {error}"),
            );
        }
    }

    fn footer(&mut self, app: &mut PhotocraftApp, ui: &mut egui::Ui) {
        self.element_inspector(app, ui);
        ui.collapsing("Lane coverage and loader messages", |ui| {
            if let Some(p) = &self.project {
                for c in &p.coverage {
                    ui.small(c);
                }
            }
            if let Some(skin) = self.preview.noteskin() {
                for note in &skin.notes {
                    ui.small(note);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.output)
                    .desired_width(ui.available_width() - 90.0),
            );
            if ui.button("Export skin").clicked() {
                let result = self
                    .project
                    .as_ref()
                    .ok_or_else(|| "Load a skin first".to_string())
                    .and_then(|p| {
                        if self.has_pending_sources() {
                            return Err(
                                "Wait for pending reloads and fix source errors before exporting"
                                    .into(),
                            );
                        }
                        p.export_texts(Path::new(&self.output), &self.edits, &self.editor.edits)
                    });
                match result {
                    Ok(()) => {
                        self.failure = None;
                        self.message = format!(
                            "Exported to {}. Import its NoteSkins folder into the game.",
                            self.output
                        );
                    }
                    Err(e) => self.failure = Some(e),
                }
            }
        });
        ui.small(format!(
            "{} artwork updates · latest {:.1} ms · gameplay renderer",
            self.updates, self.update_ms
        ));
    }

    fn restore_documents(&mut self, app: &mut PhotocraftApp) {
        if !self.restoring {
            self.restoring = true;
            for (_, path) in &self.restore {
                if !app
                    .session
                    .documents()
                    .iter()
                    .any(|s| s.path.as_deref() == path.to_str())
                    && let Err(error) = app.open_path(&path.to_string_lossy())
                {
                    self.failure = Some(format!("Could not restore {}: {error}", path.display()));
                }
            }
        }
        self.restore.retain(|(source, path)| {
            if self.bindings.contains_key(source) {
                return false;
            }
            if let Some(doc) = app
                .session
                .documents()
                .iter()
                .find(|s| s.path.as_deref() == path.to_str())
            {
                self.bindings.insert(
                    source.clone(),
                    Binding {
                        doc: doc.doc.id,
                        seen: None,
                    },
                );
                false
            } else {
                true
            }
        });
    }

    fn vector_active(&self) -> bool {
        #[cfg(feature = "skincraft")]
        if self.vector.as_ref().is_some_and(|v| v.active) {
            return true;
        }
        false
    }

    fn vector_count(&self) -> usize {
        #[cfg(feature = "skincraft")]
        if let Some(v) = &self.vector {
            return v.retained.len();
        }
        0
    }

    fn has_pending_sources(&self) -> bool {
        #[cfg(feature = "skincraft")]
        if self.vector.as_ref().is_some_and(|v| !v.errors.is_empty()) {
            return true;
        }
        self.text_reload.is_some() || self.editor.edits != self.applied_texts
    }

    fn save_project(&mut self, app: &PhotocraftApp) -> Result<(), String> {
        #[cfg(feature = "skincraft")]
        self.vector_tick();
        let Some(home) = self.home.clone() else {
            return Ok(());
        };
        let mut bindings: Vec<_> = self
            .bindings
            .iter()
            .filter_map(|(source, b)| {
                let path = app
                    .session
                    .documents()
                    .iter()
                    .find(|s| s.doc.id == b.doc)?
                    .path
                    .as_ref()?;
                Some((source.clone(), PathBuf::from(path)))
            })
            .collect();
        // A missing or still-opening old source must not prevent new bindings
        // from being saved. Keep it pending until its texture is rebound.
        bindings.extend(
            self.restore
                .iter()
                .filter(|(source, _)| !self.bindings.contains_key(source))
                .cloned(),
        );
        bindings.sort();
        let Some(folder) = self
            .project
            .as_ref()
            .map(|p| p.dir.to_string_lossy().into_owned())
        else {
            return Ok(());
        };
        self.checkpoint(app)?;
        let data = serde_json::to_vec_pretty(&SavedProject {
            folder,
            layout: self.options.layout.clone(),
            bindings,
        })
        .map_err(|e| e.to_string())?;
        if data != self.saved {
            std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
            let temp = home.join("project.json.tmp");
            std::fs::write(&temp, &data).map_err(|e| e.to_string())?;
            std::fs::rename(&temp, home.join("project.json")).map_err(|e| e.to_string())?;
            self.saved = data;
        }
        Ok(())
    }
}

impl PanelExtension for Panel {
    fn show(&mut self, app: &mut PhotocraftApp, ui: &mut egui::Ui) {
        self.restore_documents(app);
        #[cfg(feature = "skincraft")]
        self.vector_tick();
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64().min(0.1);
        self.last = now;
        if self.collapsed {
            if let Err(error) = self.update_artwork(app) {
                self.failure = Some(error);
            }
            self.workspace_tick(app, ui.ctx());
            if self.checkpoint_dirty && self.checkpoint_at.elapsed().as_secs_f32() >= 1.0 {
                if let Err(error) = self.save_project(app) {
                    self.failure = Some(error);
                }
                self.checkpoint_at = Instant::now();
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
            egui::Panel::right("noteskin-preview-collapsed")
                .exact_size(34.0)
                .show(ui, |ui| {
                    if ui
                        .button("◀")
                        .on_hover_text("Show Noteskin Preview")
                        .clicked()
                    {
                        self.collapsed = false;
                    }
                });
            return;
        }
        if self.running {
            self.time += dt;
        }
        self.workspace_browser(app, ui);
        egui::Panel::right("noteskin-preview")
            .default_size(390.0)
            .size_range(300.0..=650.0)
            .resizable(true)
            .show(ui, |ui| {
                self.controls(app, ui);
                if !self.loaded
                    && let Err(error) = self.load()
                {
                    self.failure = Some(error);
                    self.loaded = true;
                }
                if let Some(reason) = &app.perf.gpu_info.lost {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("Preview GPU lost: {reason}. Save your artwork and restart."),
                    );
                } else {
                    if let Err(error) = self.update_artwork(app) {
                        self.failure = Some(error);
                    }
                    self.draw(ui);
                }
                self.footer(app, ui);
            });
        self.workspace_tick(app, ui.ctx());
        if self.checkpoint_dirty && self.checkpoint_at.elapsed().as_secs_f32() >= 1.0 {
            if let Err(error) = self.save_project(app) {
                self.failure = Some(format!("Project checkpoint failed: {error}"));
            }
            self.checkpoint_at = Instant::now();
        }
        if self.running || !self.bindings.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn inspect(&self) -> serde_json::Value {
        serde_json::json!({"kind": "noteskin", "folder": self.folder, "layout": self.layout,
            "time": self.time, "frames": self.preview.frames(), "updates": self.updates,
            "autoplayFlashes": self.options.autoplay_flashes,
            "ruleset": self.options.rules.id,
            "updateMs": self.update_ms, "failure": self.failure, "textures": self.assets.len(),
            "selected": self.selected, "bindings": self.bindings.len(),
            "coverage": self.project.as_ref().map(|p| &p.coverage),
            "files": self.files.iter().map(|f| serde_json::json!({"path":f.path,"owner":f.owner,"inherited":f.inherited,"image":f.image,"text":f.text})).collect::<Vec<_>>(),
            "drawingDocuments": self.drawings.len(),
            "vectorDocuments": self.vector_count(), "textEdits": self.editor.edits.len(),
            "textTabs": self.editor.tabs, "reloads": self.reloads, "checkpointPending":self.checkpoint_dirty})
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        if let Some((_, id)) = self.registered {
            self.rs.renderer.write().free_texture(&id);
        }
    }
}

/// The editor's artwork, never its checkerboard, monitor transform or selection overlays.
fn composite(doc: &photocraft_doc::Document, size: [u32; 2]) -> Result<Rgba, String> {
    if doc.size.width == 0
        || doc.size.height == 0
        || doc.size.width > 4096
        || doc.size.height > 4096
        || doc.size.area() > 4 << 20
    {
        return Err("Live artwork must fit within 4096 px per side and 4 megapixels".into());
    }
    let buffer = photocraft_compose::render(doc, doc.bounds());
    let profile = photocraft_engine::color_cmds::composite_profile(doc);
    let transform = photocraft_cms::cached(
        &profile,
        photocraft_cms::Builtin::Srgb.profile(),
        Default::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut converted = vec![0.0; buffer.px.len() * 4];
    let values: Vec<f32> = buffer.px.iter().flatten().copied().collect();
    transform.convert_f32(&values, 4, &mut converted, 4, true);
    let data: Vec<u8> = converted
        .into_iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
        .collect();
    let original = [doc.size.width, doc.size.height];
    if original == size {
        return Ok(Rgba { size, data });
    }
    let image = image::RgbaImage::from_raw(original[0], original[1], data)
        .ok_or("Invalid artwork dimensions")?;
    let fit = image::imageops::resize(
        &image,
        size[0],
        size[1],
        image::imageops::FilterType::Triangle,
    );
    Ok(Rgba {
        size,
        data: fit.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App;
    use photocraft_ui_egui::Services;

    struct Host {
        app: PhotocraftApp,
        panel: Panel,
    }
    impl App for Host {
        fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
            self.app.logic(ctx, frame);
        }
        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            self.panel.show(&mut self.app, ui);
            self.app.ui(ui, frame);
        }
    }

    pub(super) fn field_pixels(panel: &Panel) -> Vec<u8> {
        let texture = panel.preview.texture().unwrap();
        let row = (texture.width() * 4).div_ceil(256) * 256;
        let buffer = panel.rs.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("authoring-test-readback"),
            size: u64::from(row * texture.height()),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = panel.rs.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(texture.height()),
                },
            },
            texture.size(),
        );
        panel.rs.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        panel
            .rs
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        slice.get_mapped_range().unwrap().to_vec()
    }

    #[test]
    fn artwork_undo_and_export_match_the_game_loader() {
        let root = crate::project::tests::fixture("live");
        let folder = root.join("dance/Test");
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1440.0, 1000.0))
            .with_pixels_per_point(1.0)
            .wgpu()
            .build_eframe(|cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let rs = cc.wgpu_render_state.clone().expect("wgpu");
                let mut app =
                    PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
                app.set_wgpu(rs.clone());
                let mut panel = Panel::new(rs);
                panel.folder = folder.to_string_lossy().into_owned();
                panel.running = false;
                panel.time = 1.0;
                panel.load().expect("skin");
                panel.selected = panel
                    .assets
                    .iter()
                    .position(|a| a.label.contains("Tap Note"))
                    .expect("tap texture");
                panel.open_artwork(&mut app).expect("editable document");
                Host { app, panel }
            });
        h.run_steps(4);
        let initial = h
            .state()
            .panel
            .edits
            .values()
            .next()
            .expect("live pixels")
            .data
            .clone();
        let drawn_initial = field_pixels(&h.state().panel);
        let time = h.state().panel.preview.demo_time();
        let count = h.state().panel.updates;
        h.run_steps(3);
        assert_eq!(
            h.state().panel.updates,
            count,
            "unchanged snapshots are cached"
        );
        h.state_mut()
            .app
            .session
            .execute("image.adjustments.invert", serde_json::json!({}))
            .expect("invert");
        h.run_steps(2);
        let edited = h
            .state()
            .panel
            .edits
            .values()
            .next()
            .expect("edited pixels")
            .data
            .clone();
        assert_ne!(initial, edited);
        let drawn_edited = field_pixels(&h.state().panel);
        assert!(
            drawn_initial != drawn_edited,
            "live pixels reach the actual playfield texture"
        );
        assert_eq!(
            h.state().panel.preview.demo_time(),
            time,
            "a texture update keeps demo time"
        );
        let out = root.join("export");
        h.state()
            .panel
            .project
            .as_ref()
            .unwrap()
            .export(&out, &h.state().panel.edits)
            .unwrap();
        let exported = Project::open(
            &out.join("NoteSkins/dance/Test"),
            &Layout::builtin("danoni-7").unwrap(),
        )
        .unwrap();
        let read = exported
            .prepare(&Layout::builtin("danoni-7").unwrap(), false)
            .unwrap();
        assert!(
            read.images.iter().flatten().any(|a| a.data == edited),
            "the game reads precisely the exported pixels"
        );
        h.state_mut()
            .app
            .session
            .execute("edit.undo", serde_json::json!({}))
            .expect("undo");
        h.run_steps(2);
        assert_eq!(h.state().panel.edits.values().next().unwrap().data, initial);
        let red_pixels = |pixels: &[u8]| {
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] > 180 && p[1] < 80 && p[2] < 110)
                .count()
        };
        let drawn_undo = field_pixels(&h.state().panel);
        assert!(
            red_pixels(&drawn_initial) > 100,
            "the test texture is visible"
        );
        assert!(
            red_pixels(&drawn_undo) > red_pixels(&drawn_edited) + 100,
            "undo restores the red artwork in the rendered playfield"
        );
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/skin-studio/synthetic-preview.png");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        h.render().expect("render").save(&path).expect("screenshot");
        assert!(h.state().panel.preview.frames() > 5);
        assert!(
            h.state().panel.failure.is_none(),
            "{:?}",
            h.state().panel.failure
        );

        // A stroke's preview key changes while its committed revision stays put.
        let revision = h.state().app.session.active().unwrap().revision;
        let ctx = h.ctx.clone();
        let (request, _) = photocraft_ui_egui::control::ControlRequest::new(
            "ui.pointer",
            serde_json::json!({
                "tool": "Brush", "events": [{"kind":"down", "x":20, "y":20}, {"kind":"move", "x":40, "y":35}]
            }),
        );
        photocraft_ui_egui::control::handle(&mut h.state_mut().app, &ctx, &request);
        h.run_steps(6);
        assert_eq!(
            h.state().app.session.active().unwrap().revision,
            revision,
            "stroke is still in progress"
        );
        assert_ne!(
            h.state().panel.edits.values().next().unwrap().data,
            initial,
            "uncommitted paint reaches the preview"
        );
        let (release, _) = photocraft_ui_egui::control::ControlRequest::new(
            "ui.pointer",
            serde_json::json!({
                "events": [{"kind":"up", "x":40, "y":35}]
            }),
        );
        photocraft_ui_egui::control::handle(&mut h.state_mut().app, &ctx, &release);
        h.run_steps(3);
        {
            let Host { app, panel } = h.state_mut();
            panel.home = Some(root.join("profile"));
            panel.restore.push((
                root.join("old-texture.png"),
                root.join("missing-source.pcraft"),
            ));
            app.session.active_mut().unwrap().path =
                Some(root.join("drawing.pcraft").to_string_lossy().into_owned());
            panel.save_project(app).unwrap();
            let saved: SavedProject =
                serde_json::from_slice(&std::fs::read(root.join("profile/project.json")).unwrap())
                    .unwrap();
            assert_eq!(
                saved.bindings.len(),
                2,
                "a missing old source does not block saving the new drawing binding"
            );
        }
        {
            let Host { app, panel } = h.state_mut();
            // Also edit an image that the current layout never loads.
            let unused = root.join("dance/Test/unused.png");
            std::fs::copy(root.join("dance/Test/Down Tap Note.png"), &unused).unwrap();
            panel.files = files::inventory(panel.project.as_ref().unwrap()).unwrap();
            let first_source = panel.assets[panel.selected].source.clone();
            let first_doc = panel.bindings[&first_source].doc;
            panel.open_image(app, &unused).unwrap();
            app.session
                .execute("layer.new.layer", serde_json::json!({"name":"Details"}))
                .unwrap();
            panel.update_artwork(app).unwrap();
            assert_eq!(
                panel.bindings.len(),
                2,
                "one instance holds multiple live drawings"
            );
            assert!(
                panel.edits.contains_key(&unused),
                "unused images are editable and exportable"
            );
            let first_pixels = panel.edits[&first_source].data.clone();
            let i = app
                .session
                .documents()
                .iter()
                .position(|d| d.doc.id == first_doc)
                .unwrap();
            app.session.close(i).unwrap();
            panel.update_artwork(app).unwrap();
            assert_eq!(
                panel.edits[&first_source].data, first_pixels,
                "closing a tab retains project pixels"
            );
            panel.open_image(app, &first_source).unwrap();
            app.session
                .execute("edit.undo", serde_json::json!({}))
                .unwrap();
            panel.update_artwork(app).unwrap();
            assert_ne!(
                panel.edits[&first_source].data, first_pixels,
                "reopened tabs keep their session undo history"
            );
            panel.save_project(app).unwrap();
            let state = workspace::load(panel.workspace_dir.as_ref().unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(state.drawings.len(), 2);
            let unused_name = &state.drawings.iter().find(|(p, _)| p == &unused).unwrap().1;
            assert_eq!(
                workspace::drawing(panel.workspace_dir.as_ref().unwrap(), unused_name)
                    .unwrap()
                    .doc
                    .layers
                    .len(),
                panel.drawings[&unused].doc.layers.len()
            );
            let mut restored = Panel::new(panel.rs.clone());
            restored.home = panel.home.clone();
            restored.folder = panel.folder.clone();
            restored.running = false;
            restored.load().unwrap();
            assert_eq!(
                restored.drawings.len(),
                2,
                "restart restores all layered sources"
            );
            assert_eq!(
                restored.edits[&first_source].data,
                panel.edits[&first_source].data
            );
            let script = root.join("dance/Test/metrics.ini");
            panel.editor.open(&script).unwrap();
            panel
                .editor
                .set(
                    &script,
                    "[Global]\nFallbackNoteSkin=missing-source\n".into(),
                )
                .unwrap();
            panel.text_reload = Some(Instant::now() - std::time::Duration::from_secs(1));
        }
        let old_reload = h.state().panel.reloads;
        let old_keys = h
            .state()
            .panel
            .preview
            .noteskin()
            .unwrap()
            .texture_keys()
            .to_vec();
        let old_frames = h.state().panel.preview.frames();
        h.run_steps(3);
        assert_eq!(
            h.state().panel.reloads,
            old_reload,
            "invalid metrics keep the last successful scene"
        );
        assert!(h.state().panel.failure.is_some());
        assert_eq!(
            h.state().panel.preview.noteskin().unwrap().texture_keys(),
            old_keys
        );
        assert!(
            h.state().panel.preview.frames() > old_frames,
            "the valid preview keeps rendering while an edit is invalid"
        );
        {
            let Host { app: _, panel } = h.state_mut();
            let script = root.join("dance/Test/metrics.ini");
            panel
                .editor
                .set(
                    &script,
                    "[Global]\nFallbackNoteSkin=default\n; edited in PhotoCraft\n".into(),
                )
                .unwrap();
            panel.text_reload = Some(Instant::now() - std::time::Duration::from_secs(1));
        }
        h.run_steps(3);
        assert!(
            h.state().panel.failure.is_none(),
            "{:?}",
            h.state().panel.failure
        );
        assert!(h.state().panel.reloads > old_reload);
        {
            let Host { app: _, panel } = h.state_mut();
            let script = root.join("dance/Test/Down Tap Note.lua");
            std::fs::write(
                &script,
                "return Def.Sprite { Texture = NOTESKIN:GetPath('Down', 'Tap Note.png') }",
            )
            .unwrap();
            panel.load().unwrap();
            panel.running = true;
            panel.editor.open(&script).unwrap();
            panel.editor.set(&script,"return Def.ActorFrame { InitCommand = function(self) self:SetUpdateFunction(function() error('draft animation error') end) end, Def.Sprite { Texture = NOTESKIN:GetPath('Down', 'Tap Note.png') } }".into()).unwrap();
            panel.text_reload = Some(Instant::now() - std::time::Duration::from_secs(1));
        }
        h.run_steps(6);
        assert!(
            h.state()
                .panel
                .failure
                .as_ref()
                .is_some_and(|e| e.contains("draft animation error")),
            "{:?}",
            h.state().panel.failure
        );
        assert!(
            h.state()
                .panel
                .preview
                .noteskin()
                .unwrap()
                .lua_failure()
                .is_none(),
            "a frame-time failure restores the last working actor tree"
        );
        assert!(
            h.state()
                .panel
                .editor
                .edits
                .values()
                .any(|s| s.contains("draft animation error")),
            "the failed draft stays editable"
        );
        {
            let Host { app: _, panel } = h.state_mut();
            let script = root.join("dance/Test/Down Tap Note.lua");
            panel
                .editor
                .set(
                    &script,
                    "return Def.Sprite { Texture = NOTESKIN:GetPath('Down', 'Tap Note.png') }"
                        .into(),
                )
                .unwrap();
            panel.text_reload = Some(Instant::now() - std::time::Duration::from_secs(1));
        }
        h.run_steps(3);
        assert!(h.state().panel.failure.is_none());
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/skin-studio/workspace.png");
        h.render().unwrap().save(path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn embedded_panel_on_a_bundled_onigiri_extension() {
        let root = crate::project::tests::fixture("bundled-onigiri");
        let layout = Layout::builtin("danoni-7").unwrap();
        let base = Project::open(
            &ddi_desktop::images::assets().join("noteskins/dance/Alpha-Arrows"),
            &layout,
        )
        .unwrap();
        let working = base
            .add_onigiri(&root.join("working"), &HashMap::new())
            .unwrap();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1440.0, 1000.0))
            .with_pixels_per_point(1.0)
            .wgpu()
            .build_eframe(|cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let rs = cc.wgpu_render_state.clone().expect("wgpu");
                let mut app =
                    PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
                app.set_wgpu(rs.clone());
                let mut panel = Panel::new(rs);
                panel.folder = working.to_string_lossy().into_owned();
                panel.running = false;
                panel.time = 1.0;
                panel.load().unwrap();
                panel.selected = panel
                    .assets
                    .iter()
                    .position(|a| a.label.contains("_studio onigiri tap"))
                    .expect("own onigiri tap");
                panel.open_artwork(&mut app).unwrap();
                panel.selected = panel
                    .assets
                    .iter()
                    .position(|a| a.label.contains("_studio onigiri receptor"))
                    .unwrap();
                panel.open_artwork(&mut app).unwrap();
                panel.search = "onigiri".into();
                panel
                    .editor
                    .open(&panel.project.as_ref().unwrap().dir.join("NoteSkin.lua"))
                    .unwrap();
                app.panel_extension = Some(Box::new(panel));
                app
            });
        h.run_steps(5);
        let seen = h.state().panel_extension.as_ref().unwrap().inspect();
        assert!(seen["frames"].as_u64().unwrap() > 2);
        assert!(seen["failure"].is_null(), "{seen}");
        assert_eq!(seen["bindings"], 2);
        assert_eq!(seen["textTabs"].as_array().unwrap().len(), 1);
        let image = h.render().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/skin-studio/preview.png");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        image.save(path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
