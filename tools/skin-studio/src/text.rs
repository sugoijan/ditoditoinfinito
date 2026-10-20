//! Small native text workspace. egui owns each file's independent edit/undo state.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Default)]
pub struct Editor {
    pub tabs: Vec<PathBuf>,
    pub active: Option<PathBuf>,
    pub edits: HashMap<PathBuf, String>,
    buffers: HashMap<PathBuf, String>,
    originals: HashMap<PathBuf, String>,
    pub changed: Option<Instant>,
    pub visible: bool,
    pub read_only: HashSet<PathBuf>,
    namespace: String,
}

impl Editor {
    pub fn for_project(path: &Path) -> Self {
        Self {
            namespace: crate::workspace::key(path),
            ..Default::default()
        }
    }

    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        if !self.buffers.contains_key(path) {
            let original = crate::files::read_text(path)?;
            let text = self.edits.get(path).unwrap_or(&original).clone();
            self.buffers.insert(path.to_path_buf(), text);
            self.originals.insert(path.to_path_buf(), original);
        }
        if !self.tabs.iter().any(|p| p == path) {
            self.tabs.push(path.to_path_buf());
        }
        self.active = Some(path.to_path_buf());
        self.visible = true;
        Ok(())
    }

    pub fn set(&mut self, path: &Path, text: String) -> Result<(), String> {
        if text.len() > 1 << 20 {
            return Err("Text exceeds 1 MiB".into());
        }
        if self.originals.get(path) == Some(&text) {
            self.edits.remove(path);
        } else {
            self.edits.insert(path.to_path_buf(), text.clone());
        }
        self.buffers.insert(path.to_path_buf(), text);
        self.changed = Some(Instant::now());
        Ok(())
    }

    pub fn show(&mut self, ctx: &egui::Context, error: Option<&str>) -> bool {
        if !self.visible {
            return false;
        }
        let mut visible = self.visible;
        let mut edited = None;
        egui::Window::new("Noteskin scripts and text")
            .id(egui::Id::new("noteskin-text-editor"))
            .open(&mut visible)
            .default_pos(egui::pos2(90.0, 100.0))
            .default_size(egui::vec2(620.0, 490.0))
            .show(ctx, |ui| {
                egui::ScrollArea::horizontal().id_salt("script-tabs").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut close = None;
                        for path in &self.tabs {
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            let label = format!("{}{}", name, if self.edits.contains_key(path) { " •" } else { "" });
                            if ui.selectable_label(self.active.as_ref() == Some(path), label).on_hover_text(path.display().to_string()).clicked() { self.active = Some(path.clone()); }
                            if ui.small_button("×").on_hover_text("Close tab; project keeps edits").clicked() { close = Some(path.clone()); }
                        }
                        if let Some(path) = close {
                            self.tabs.retain(|p| p != &path);
                            if self.active.as_ref() == Some(&path) { self.active = self.tabs.last().cloned(); }
                        }
                    });
                });
                ui.separator();
                if let Some(error) = error { ui.colored_label(ui.visuals().error_fg_color, format!("Last working preview retained: {error}")); }
                if let Some(path) = &self.active {
                    ui.small(path.display().to_string());
                    if self.read_only.contains(path) {ui.small("This SVG is managed by the vector editor. Use SVG source in the file browser to switch to text editing.");}
                    if let Some(text) = self.buffers.get_mut(path) {
                        let language = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                        let mut layouter = |ui: &egui::Ui, value: &dyn egui::TextBuffer, _width: f32| {
                            let job = highlight(value.as_str(), language, ui.visuals());
                            ui.fonts_mut(|f| f.layout_job(job))
                        };
                        let response = egui::ScrollArea::both().id_salt(("source-scroll",path)).max_height(ui.available_height() - 28.0).show(ui, |ui| {
                            ui.add(egui::TextEdit::multiline(text).id_salt(("source",&self.namespace,path)).code_editor().interactive(!self.read_only.contains(path)).desired_width(f32::INFINITY).desired_rows(18).char_limit(1 << 20).layouter(&mut layouter))
                        }).inner;
                        let cursor = egui::TextEdit::load_state(ctx, response.id).and_then(|s| s.cursor.char_range()).map(|r| usize::from(r.primary.index)).unwrap_or(0);
                        let line = text.chars().take(cursor).filter(|c| *c == '\n').count()+1;
                        ui.small(format!("Line {line} · {} lines · Undo/redo: ⌘Z / ⇧⌘Z · reload after typing pauses", text.lines().count().max(1)));
                        if response.changed() { edited = Some((path.clone(), text.clone())); }
                    }
                } else { ui.label("Open a script, metrics file or SVG source from Noteskin Files."); }
            });
        self.visible = visible;
        if let Some((path, text)) = edited {
            return self.set(&path, text).is_ok();
        }
        false
    }
}

fn highlight(text: &str, language: &str, visuals: &egui::Visuals) -> egui::text::LayoutJob {
    let normal = visuals.text_color();
    let keyword = visuals.selection.stroke.color;
    let string = egui::Color32::from_rgb(140, 190, 135);
    let comment = visuals.weak_text_color();
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::monospace(13.0);
    let mut rest = text;
    while !rest.is_empty() {
        let first = rest.chars().next().unwrap_or(' ');
        let (length, color) = if rest.starts_with("--") && language.eq_ignore_ascii_case("lua")
            || rest.starts_with(';') && language.eq_ignore_ascii_case("ini")
        {
            (rest.find('\n').unwrap_or(rest.len()), comment)
        } else if first == '\'' || first == '"' {
            let mut escaped = false;
            let mut end = rest.len();
            for (i, c) in rest.char_indices().skip(1) {
                if c == first && !escaped {
                    end = i + c.len_utf8();
                    break;
                }
                escaped = c == '\\' && !escaped;
            }
            (end, string)
        } else if first.is_alphanumeric() || first == '_' {
            let end = rest
                .char_indices()
                .find(|(_, c)| !c.is_alphanumeric() && *c != '_')
                .map_or(rest.len(), |(i, _)| i);
            let token = rest.get(..end).unwrap_or_default();
            let color = if matches!(
                token,
                "return"
                    | "function"
                    | "local"
                    | "end"
                    | "if"
                    | "then"
                    | "else"
                    | "elseif"
                    | "for"
                    | "while"
                    | "do"
                    | "true"
                    | "false"
                    | "nil"
                    | "and"
                    | "or"
                    | "not"
                    | "in"
            ) {
                keyword
            } else {
                normal
            };
            (end, color)
        } else {
            (first.len_utf8(), normal)
        };
        if length == 0 {
            break;
        }
        if let Some(token) = rest.get(..length) {
            job.append(
                token,
                0.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color,
                    ..Default::default()
                },
            );
        }
        rest = rest.get(length..).unwrap_or_default();
    }
    job
}
