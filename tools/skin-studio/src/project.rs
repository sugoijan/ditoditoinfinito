use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ddi_chart::Layout;
use ddi_desktop::library::noteskins::DiskSkin;
use ddi_desktop::play::{skins, view::Rgba};
use ddi_noteskin::build::TextureKey;
use ddi_noteskin::{Dir, Skin};

#[derive(Clone)]
pub struct Project {
    pub dir: PathBuf,
    pub skins: Vec<DiskSkin>,
    pub chain: Vec<usize>,
    pub coverage: Vec<String>,
    layout: String,
}

pub struct Asset {
    pub source: PathBuf,
    pub label: String,
    pub image: Rgba,
    pub texture: eframe::wgpu::Texture,
    pub mipmaps: bool,
}

pub fn choices() -> Vec<PathBuf> {
    let base = ddi_desktop::images::assets().join("noteskins");
    let mut out = Vec::new();
    for game in ["dance", "pump", "common"] {
        if let Ok(entries) = std::fs::read_dir(base.join(game)) {
            out.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
    }
    out.sort();
    out
}

fn disk(dir: &Path) -> Result<DiskSkin, String> {
    let dir = dir.canonicalize().map_err(|e| e.to_string())?;
    let name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Skin folder needs a Unicode name")?
        .to_string();
    let game = dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .filter(|s| ["dance", "pump", "common"].contains(s))
        .unwrap_or("dance")
        .to_string();
    Ok(DiskSkin {
        game,
        name,
        dir,
        bundled: false,
        variant_of: None,
    })
}

impl Project {
    pub fn open(dir: &Path, layout: &Layout) -> Result<Self, String> {
        let chosen = disk(dir)?;
        if !chosen.dir.join("metrics.ini").is_file() && !chosen.dir.join("NoteSkin.lua").is_file() {
            return Err("Choose a skin folder containing metrics.ini or NoteSkin.lua".into());
        }
        let mut candidates = vec![chosen.dir.clone()];
        if let Some(parent) = chosen.dir.parent()
            && let Ok(entries) = std::fs::read_dir(parent)
        {
            candidates.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
        candidates.extend(choices());
        let mut skins = Vec::<DiskSkin>::new();
        for path in candidates {
            if let Ok(s) = disk(&path)
                && !skins.iter().any(|old| old.key() == s.key())
            {
                skins.push(s);
            }
        }
        let folders: Vec<_> = skins.iter().map(skins::folder).collect();
        let skin = Skin::load(&folders, &chosen.game, &chosen.name).map_err(|e| e.to_string())?;
        let mut chain = Vec::new();
        for d in &skin.chain {
            if let Dir::Folder(i) = *d
                && !chain.contains(&i)
            {
                chain.push(i);
            }
        }
        let coverage = layout
            .lanes
            .iter()
            .map(|lane| {
                let button = skin
                    .lane_button(lane)
                    .map(|b| b.button.name())
                    .unwrap_or("unavailable");
                format!("{} → {button}", lane.name)
            })
            .collect();
        Ok(Self {
            dir: chosen.dir,
            skins,
            chain,
            coverage,
            layout: layout.id.clone(),
        })
    }

    pub fn prepare(&self, layout: &Layout, reversed: bool) -> Result<skins::Prepared, String> {
        let chosen = self.skins.first().ok_or("No skin selected")?;
        let env = ddi_engine::demo::song(&layout.id, ddi_render::field_preview::BPM)
            .and_then(|song| {
                song.charts
                    .first()
                    .map(|chart| skins::lua_env(&song, chart, layout, reversed))
            })
            .unwrap_or_else(|| ddi_noteskin::lua::Env {
                reversed,
                columns: layout.lanes.len() as u32,
                ..Default::default()
            });
        let prepared = skins::prepare_authoring(&chosen.key(), layout, env, &self.skins)
            .ok_or_else(|| {
                "The game's loader could not build this skin; check its Lua and fallback folders"
                    .to_string()
            })?;
        if let Some(error) = &prepared.load_error {
            return Err(error.clone());
        }
        Ok(prepared)
    }

    pub fn source(&self, key: &TextureKey) -> Option<PathBuf> {
        match key {
            TextureKey::Skin {
                dir: Dir::Folder(i),
                path,
            } => self.skins.get(*i).map(|s| s.dir.join(path)),
            _ => None,
        }
    }

    /// Write a new NoteSkins tree. Existing destinations are never overwritten.
    pub fn export(&self, out: &Path, edits: &HashMap<PathBuf, Rgba>) -> Result<(), String> {
        if out.exists() {
            return Err("Export needs a new folder; choose a name that does not exist".into());
        }
        let parent = out.parent().ok_or("Export folder has no parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = out
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid export folder")?;
        let stage = parent.join(format!(".{name}.ddi-export-{}", std::process::id()));
        std::fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let result = self.write_tree(&stage, edits).and_then(|()| {
            self.validate_export(&stage)?;
            if out.exists() {
                return Err("Export destination was created while exporting".into());
            }
            std::fs::rename(&stage, out).map_err(|e| e.to_string())
        });
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&stage);
        }
        result
    }

    #[cfg(test)]
    pub fn add_onigiri(
        &self,
        out: &Path,
        edits: &HashMap<PathBuf, Rgba>,
    ) -> Result<PathBuf, String> {
        self.add_onigiri_texts(out, edits, &HashMap::new())
    }

    pub fn add_onigiri_texts(
        &self,
        out: &Path,
        edits: &HashMap<PathBuf, Rgba>,
        texts: &HashMap<PathBuf, String>,
    ) -> Result<PathBuf, String> {
        let plan = self.text_chain(texts)?;
        let base = self.skins.first().ok_or("No skin selected")?;
        if base.name.contains(['\n', '\r', '[', ']']) {
            return Err("Unsupported skin name in fallback metrics".into());
        }
        // Build and validate everything before exposing the working copy.
        let parent = out.parent().ok_or("Working folder needs a parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        if out.exists() {
            return Err("Choose a new working folder".into());
        }
        let stage = parent.join(format!(".ddi-onigiri-{}", std::process::id()));
        std::fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let result = (|| {
            plan.write_tree_with(&stage, edits, texts)?;
            let name = format!("{} Studio", base.name);
            let rel = Path::new("NoteSkins").join(&base.game).join(&name);
            let target = stage.join(&rel);
            std::fs::create_dir(&target).map_err(|e| e.to_string())?;
            let mut metrics = format!("[Global]\nFallbackNoteSkin={}\n\n[Onigiri]\n", base.name);
            for part in ["TapNote", "TapLift", "TapFake", "HoldHead"] {
                metrics.push_str(&format!("{part}NoteColorTextureCoordSpacingX=0\n{part}NoteColorTextureCoordSpacingY=0\n{part}AnimationIsNoteColor=0\n{part}AnimationIsVivid=0\n"));
            }
            std::fs::write(target.join("metrics.ini"), metrics).map_err(|e| e.to_string())?;
            for (name, content) in [
                ("NoteSkin.lua", include_str!("onigiri-loader.lua")),
                ("Onigiri Tap Note.lua", include_str!("onigiri-tap.lua")),
                ("Onigiri Receptor.lua", include_str!("onigiri-receptor.lua")),
                (
                    "Onigiri Explosion.lua",
                    include_str!("onigiri-explosion.lua"),
                ),
            ] {
                std::fs::write(target.join(name), content).map_err(|e| e.to_string())?;
            }
            for receptor in [false, true] {
                let label = if receptor { "receptor" } else { "tap" };
                let image = onigiri_template(receptor);
                std::fs::write(
                    target.join(format!("_studio onigiri {label} (res 64x64).png")),
                    png_bytes(&image)?,
                )
                .map_err(|e| e.to_string())?;
            }
            std::fs::write(target.join("AUTHORING.txt"), "Original onigiri starter artwork and Lua: MIT, Dito Dito Infinito contributors.\nInherited artwork retains its upstream licences.\n").map_err(|e| e.to_string())?;
            std::fs::write(
                target.join("LICENSE-MIT.txt"),
                include_str!("../../../LICENSES/MIT.txt"),
            )
            .map_err(|e| e.to_string())?;
            let layout = Layout::builtin("danoni-7").ok_or("7-key layout unavailable")?;
            let check = Self::open(&target, &layout)?;
            let loaded = check.prepare(&layout, false)?;
            if let Some(reason) = loaded.play.lua_failure() {
                return Err(reason.to_string());
            }
            if out.exists() {
                return Err("Working folder already exists".into());
            }
            std::fs::rename(&stage, out).map_err(|e| e.to_string())?;
            Ok(out.join(rel))
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&stage);
        }
        result
    }

    fn write_tree(&self, out: &Path, edits: &HashMap<PathBuf, Rgba>) -> Result<(), String> {
        self.write_tree_with(out, edits, &HashMap::new())
    }

    fn write_tree_with(
        &self,
        out: &Path,
        edits: &HashMap<PathBuf, Rgba>,
        text_edits: &HashMap<PathBuf, String>,
    ) -> Result<(), String> {
        let mut bytes = 0u64;
        let mut replacements = Vec::<(String, String)>::new();
        let mut texts = Vec::<PathBuf>::new();
        let mut applied = std::collections::HashSet::new();
        let mut applied_texts = std::collections::HashSet::new();
        let mut destinations = std::collections::HashSet::new();
        for i in &self.chain {
            let s = self.skins.get(*i).ok_or("Invalid fallback folder")?;
            let target = out.join("NoteSkins").join(&s.game).join(&s.name);
            let files = ddi_desktop::library::files::walk(&s.dir);
            if !files.unread.is_empty() {
                return Err(format!("Cannot export every file: {:?}", files.unread));
            }
            for f in files.listed {
                bytes = bytes.checked_add(f.size).ok_or("Skin too large")?;
                if bytes > 512 << 20 {
                    return Err("Export exceeds 512 MiB".into());
                }
                let from = s.dir.join(f.path.as_str());
                let to = target.join(f.path.as_str());
                if let Some(p) = to.parent() {
                    std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
                if edits.contains_key(&from) && text_edits.contains_key(&from) {
                    return Err(format!(
                        "{} has both drawing and source edits. Choose one representation before exporting",
                        from.display()
                    ));
                }
                if let Some(image) = edits.get(&from) {
                    applied.insert(from.clone());
                    let png = png_destination(&from, &to)?;
                    if !destinations.insert(png.to_string_lossy().to_lowercase()) {
                        return Err(format!(
                            "Raster export would overwrite another texture: {}",
                            png.display()
                        ));
                    }
                    std::fs::write(&png, png_bytes(image)?).map_err(|e| e.to_string())?;
                    if png != to {
                        let old = f.path.as_str().to_string();
                        let new = png
                            .strip_prefix(&target)
                            .map_err(|e| e.to_string())?
                            .to_string_lossy()
                            .replace('\\', "/");
                        replacements.push((old.clone(), new.clone()));
                        if let (Some(a), Some(b)) =
                            (Path::new(&old).file_name(), Path::new(&new).file_name())
                        {
                            replacements
                                .push((a.to_string_lossy().into(), b.to_string_lossy().into()));
                        }
                    }
                } else {
                    if !destinations.insert(to.to_string_lossy().to_lowercase()) {
                        return Err(format!("Export filenames collide: {}", to.display()));
                    }
                    if let Some(text) = text_edits.get(&from) {
                        if text.len() > 1 << 20 {
                            return Err("Text exceeds 1 MiB".into());
                        }
                        std::fs::write(&to, text).map_err(|e| e.to_string())?;
                        applied_texts.insert(from.clone());
                    } else {
                        std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
                    }
                }
                if ddi_noteskin::import::kept_text(f.path.as_str()) {
                    texts.push(to);
                }
            }
        }
        if applied_texts.len() != text_edits.len() {
            return Err("An edited text no longer belongs to the fallback chain".into());
        }
        if applied.len() != edits.len() {
            return Err("An edited source no longer belongs to this skin's fallback chain; reload it before exporting".into());
        }
        // Explicit file references follow a rasterized SVG/GIF/JPEG to its PNG.
        // Extensionless GetPath lookups already find the new file by the same stem.
        for file in texts {
            let data = std::fs::read(&file).map_err(|e| e.to_string())?;
            if let Ok(mut text) = String::from_utf8(data) {
                for (old, new) in &replacements {
                    text = text.replace(old, new);
                    // GetPath may split a literal filename between button and element.
                    let old_name = Path::new(old)
                        .file_name()
                        .and_then(|p| p.to_str())
                        .unwrap_or(old);
                    let new_name = Path::new(new)
                        .file_name()
                        .and_then(|p| p.to_str())
                        .unwrap_or(new);
                    for (at, _) in old_name.char_indices().filter(|(_, c)| *c == ' ') {
                        if let Some(suffix) = new_name.strip_prefix(&old_name[..=at]) {
                            let old_suffix = &old_name[at + 1..];
                            for quote in ['\'', '"'] {
                                text = text.replace(
                                    &format!("{quote}{old_suffix}{quote}"),
                                    &format!("{quote}{suffix}{quote}"),
                                );
                            }
                        }
                    }
                }
                std::fs::write(&file, text).map_err(|e| e.to_string())?;
            }
        }
        let changes: Vec<_> = edits.keys().map(|p| p.to_string_lossy()).collect();
        std::fs::write(out.join("AUTHORING.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "tool": "Dito Dito Infinito Noteskin Studio", "source": self.dir,
            "modifiedTextures": changes, "modifiedTexts": text_edits.keys().collect::<Vec<_>>(), "conversion": "Edited composites exported as straight-alpha sRGB PNG",
            "notice": "Upstream notices are retained. Update PROVENANCE.toml before bundling these modifications."
        })).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    /// Resolve fallback changes using the edited metrics before copying the candidate tree.
    fn text_chain(&self, texts: &HashMap<PathBuf, String>) -> Result<Self, String> {
        let mut folders: Vec<_> = self.skins.iter().map(skins::folder).collect();
        for (i, disk) in self.skins.iter().enumerate() {
            if let Some(folder) = folders.get_mut(i) {
                for (name, contents) in &mut folder.texts {
                    if let Some(edit) = texts.get(&disk.dir.join(name)) {
                        *contents = edit.clone();
                    }
                }
            }
        }
        let chosen = self.skins.first().ok_or("Missing skin")?;
        let skin = Skin::load(&folders, &chosen.game, &chosen.name).map_err(|e| e.to_string())?;
        let mut next = self.clone();
        next.chain.clear();
        for dir in &skin.chain {
            if let Dir::Folder(i) = dir
                && !next.chain.contains(i)
            {
                next.chain.push(*i);
            }
        }
        for i in &self.chain {
            if !next.chain.contains(i) {
                next.chain.push(*i);
            }
        }
        // Preserve edited inherited sources even if an edited fallback no longer uses them.
        for (i, disk) in self.skins.iter().enumerate() {
            if texts.keys().any(|p| p.starts_with(&disk.dir)) && !next.chain.contains(&i) {
                next.chain.push(i);
            }
        }
        Ok(next)
    }

    /// Build through the ordinary game loader in a private candidate tree. No source is written.
    /// Returned folder identities are remapped to the original files, so live image bindings survive.
    pub fn prepare_texts(
        &self,
        layout: &Layout,
        reverse: bool,
        texts: &HashMap<PathBuf, String>,
        stage: &Path,
    ) -> Result<(Self, skins::Prepared), String> {
        if texts.is_empty() {
            return Ok((self.clone(), self.prepare(layout, reverse)?));
        }
        let plan = self.text_chain(texts)?;
        let baseline = self.prepare(layout, reverse)?;
        if let Some(parent) = stage.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir(stage).map_err(|e| e.to_string())?;
        let result = (|| {
            plan.write_tree_with(stage, &HashMap::new(), texts)?;
            let selected = self.skins.first().ok_or("Missing skin")?;
            let mut project = Self::open(
                &stage
                    .join("NoteSkins")
                    .join(&selected.game)
                    .join(&selected.name),
                layout,
            )?;
            let prepared = project.prepare(layout, reverse)?;
            let new_errors: Vec<_> = prepared
                .play
                .notes
                .iter()
                .filter(|n| !baseline.play.notes.contains(n))
                .cloned()
                .collect();
            if !new_errors.is_empty() {
                return Err(new_errors.join("\n"));
            }
            // Images have been decoded and Lua owns its texts; it no longer reads these paths.
            for disk in &mut project.skins {
                if let Some(original) = self
                    .skins
                    .iter()
                    .find(|s| s.game == disk.game && s.name == disk.name)
                {
                    disk.dir = original.dir.clone();
                }
            }
            for index in &plan.chain {
                if let Some(original) = plan.skins.get(*index)
                    && let Some(i) = project.skins.iter().position(|s| s.dir == original.dir)
                    && !project.chain.contains(&i)
                {
                    project.chain.push(i);
                }
            }
            project.dir = self.dir.clone();
            Ok((project, prepared))
        })();
        let _ = std::fs::remove_dir_all(stage);
        result
    }

    pub fn export_texts(
        &self,
        out: &Path,
        edits: &HashMap<PathBuf, Rgba>,
        texts: &HashMap<PathBuf, String>,
    ) -> Result<(), String> {
        if texts.is_empty() {
            return self.export(out, edits);
        }
        if out.exists() {
            return Err("Export needs a new destination folder".into());
        }
        let (_, before) = self.prepare_texts(
            &Layout::builtin(&self.layout).ok_or("Unknown layout")?,
            false,
            texts,
            &out.with_extension("validate-text"),
        )?;
        let parent = out.parent().ok_or("Export needs a parent folder")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let stage = parent.join(format!(".ddi-export-text-{}", std::process::id()));
        std::fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let result = (|| {
            self.text_chain(texts)?
                .write_tree_with(&stage, edits, texts)?;
            let selected = self.skins.first().ok_or("Missing skin")?;
            let p = Self::open(
                &stage
                    .join("NoteSkins")
                    .join(&selected.game)
                    .join(&selected.name),
                &Layout::builtin(&self.layout).ok_or("Unknown layout")?,
            )?;
            let ready = p.prepare(
                &Layout::builtin(&self.layout).ok_or("Unknown layout")?,
                false,
            )?;
            if ready.play.runs_lua() != before.play.runs_lua()
                || ready.images.iter().flatten().count() < before.images.iter().flatten().count()
                || ready
                    .play
                    .notes
                    .iter()
                    .any(|n| !before.play.notes.contains(n))
            {
                return Err("The exported skin gained loader errors after rasterization; check its script image references".into());
            }
            if out.exists() {
                return Err("Export destination was created while exporting".into());
            }
            std::fs::rename(&stage, out).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(stage);
        }
        result
    }

    fn validate_export(&self, root: &Path) -> Result<(), String> {
        let selected = self.skins.first().ok_or("No skin selected")?;
        let layout = Layout::builtin(&self.layout).ok_or("Unknown export layout")?;
        let before = self.prepare(&layout, false)?;
        let after = Self::open(
            &root
                .join("NoteSkins")
                .join(&selected.game)
                .join(&selected.name),
            &layout,
        )?
        .prepare(&layout, false)?;
        if before.play.runs_lua() != after.play.runs_lua()
            || after.images.iter().flatten().count() < before.images.iter().flatten().count()
            || after
                .play
                .notes
                .iter()
                .any(|n| !before.play.notes.contains(n))
        {
            return Err("The exported skin gained loader errors. Check explicit image references and filenames; the export was not committed.".into());
        }
        Ok(())
    }
}

fn png_destination(source: &Path, destination: &Path) -> Result<PathBuf, String> {
    // SVGs are rasterized at 4× by the game's loader. The PNG must retain
    // the SVG's logical size, including the whole size of a frame sheet.
    // Otherwise a hand-edited Dito arrow would become four times as large.
    if source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
        && ddi_noteskin::hints::Hints::parse(&source.to_string_lossy())
            .res
            .is_none()
    {
        let metadata = std::fs::metadata(source).map_err(|e| e.to_string())?;
        if metadata.len() > ddi_noteskin::svg::MAX_SVG as u64 {
            return Err("SVG is too large".into());
        }
        let text = std::fs::read_to_string(source).map_err(|e| e.to_string())?;
        let (w, h) = ddi_noteskin::svg::check(&text)?;
        let stem = destination
            .file_stem()
            .and_then(|n| n.to_str())
            .ok_or("Invalid image filename")?;
        return Ok(destination.with_file_name(format!("{stem} (res {w}x{h}).png")));
    }
    Ok(destination.with_extension("png"))
}

pub fn png_bytes(image: &Rgba) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &image.data,
            image.size[0],
            image.size[1],
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// A neutral rice triangle to draw over, at four times the game's 64 px size.
fn onigiri_template(receptor: bool) -> Rgba {
    let size = 256u32;
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let px = x as f32 / 4.0;
            let py = y as f32 / 4.0;
            let half = (py - 6.0) * 0.57;
            let inside = (8.0..=55.0).contains(&py) && (px - 32.0).abs() <= half;
            if !inside {
                continue;
            }
            let edge = py > 51.0 || (px - 32.0).abs() > half - 3.0;
            let nori = (25.0..=39.0).contains(&px) && (37.0..=55.0).contains(&py);
            let color = if receptor {
                if edge {
                    [190, 200, 220, 255]
                } else {
                    [0, 0, 0, 0]
                }
            } else if nori {
                [35, 65, 57, 255]
            } else if edge {
                [180, 199, 211, 255]
            } else {
                [248, 247, 235, 255]
            };
            let i = ((y * size + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&color);
        }
    }
    Rgba {
        size: [size, size],
        data,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    pub fn fixture(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ddi-studio-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let dir = root.join("dance/Test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=default\n",
        )
        .unwrap();
        let image = Rgba {
            size: [64, 64],
            data: [240, 40, 70, 255].repeat(64 * 64),
        };
        std::fs::write(dir.join("Down Tap Note.png"), png_bytes(&image).unwrap()).unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn onigiri_extension_preserves_original_and_loads_its_own_art() {
        let root = fixture("onigiri");
        let layout = Layout::builtin("danoni-7").unwrap();
        let p = Project::open(&root.join("dance/Test"), &layout).unwrap();
        assert!(p.coverage.iter().any(|s| s.ends_with("→ Down")));
        let output = root.join("working");
        let child = p.add_onigiri(&output, &HashMap::new()).unwrap();
        let p = Project::open(&child, &layout).unwrap();
        assert!(
            p.coverage.iter().any(|s| s.ends_with("→ Onigiri")),
            "{:?}",
            p.coverage
        );
        let read = p.prepare(&layout, false).unwrap();
        assert!(read.play.runs_lua());
        assert!(read.play.texture_keys().iter().any(
            |k| matches!(k, TextureKey::Skin {path, ..} if path.contains("_studio onigiri tap"))
        ));
        assert!(read.play.lua_failure().is_none());
        assert_eq!(
            std::fs::read_to_string(root.join("dance/Test/metrics.ini")).unwrap(),
            "[Global]\nFallbackNoteSkin=default\n"
        );
        assert!(
            p.export(&output, &HashMap::new()).is_err(),
            "cannot overwrite existing folders"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn edited_svg_keeps_its_logical_size_and_explicit_references() {
        let root = fixture("svg");
        let dir = root.join("dance/Test");
        std::fs::remove_file(dir.join("Down Tap Note.png")).unwrap();
        std::fs::write(
            dir.join("Down Tap Note.lua"),
            "return Def.Sprite { Texture = NOTESKIN:GetPath('_note', 'tap.svg') }",
        )
        .unwrap();
        let svg = dir.join("_note tap.svg");
        std::fs::write(&svg, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"><rect width=\"64\" height=\"64\" fill=\"#ffffff\"/></svg>").unwrap();
        let layout = Layout::dance_single();
        let p = Project::open(&dir, &layout).unwrap();
        let original = p.prepare(&layout, false).unwrap();
        let image = original.images.iter().flatten().next().unwrap().clone();
        assert_eq!(image.size, [256, 256]);
        let edits = HashMap::from([(svg.canonicalize().unwrap(), image)]);
        let out = root.join("export");
        p.export(&out, &edits).unwrap();
        let target = out.join("NoteSkins/dance/Test");
        let file = "_note tap (res 64x64).png";
        assert!(target.join(file).is_file());
        let hints = ddi_noteskin::hints::Hints::parse(file).sheet(256, 256, 4096);
        assert_eq!(hints.frame_size(), (64.0, 64.0));
        assert!(
            std::fs::read_to_string(target.join("Down Tap Note.lua"))
                .unwrap()
                .contains("tap (res 64x64).png")
        );
        let roundtrip = Project::open(&target, &layout)
            .unwrap()
            .prepare(&layout, false)
            .unwrap();
        assert!(
            roundtrip
                .play
                .texture_keys()
                .iter()
                .any(|k| matches!(k, TextureKey::Skin {path, ..} if path == file))
        );
        assert!(roundtrip.play.lua_failure().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dito_sheet_exports_at_the_same_note_size() {
        let root = fixture("dito");
        let layout = Layout::builtin("danoni-7").unwrap();
        let p = Project::open(
            &ddi_desktop::images::assets().join("noteskins/dance/Dito"),
            &layout,
        )
        .unwrap();
        let read = p.prepare(&layout, false).unwrap();
        let (key, image) = read
            .play
            .texture_keys()
            .iter()
            .zip(&read.images)
            .find_map(|(key, image)| match key {
                TextureKey::Skin { path, .. } if path.contains("_down tap note 1x16") => {
                    Some((key, image.as_ref().unwrap().clone()))
                }
                _ => None,
            })
            .unwrap();
        let edits = HashMap::from([(p.source(key).unwrap(), image)]);
        let out = root.join("export");
        p.export(&out, &edits).unwrap();
        let file = "_down tap note 1x16 (res 64x1024).png";
        assert!(out.join("NoteSkins/dance/Dito").join(file).is_file());
        let hints = ddi_noteskin::hints::Hints::parse(file).sheet(256, 4096, 4096);
        assert_eq!(hints.frame_size(), (64.0, 64.0));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn inventory_and_text_drafts_cover_inherited_and_unused_files() {
        let root = fixture("text-project");
        let skin = root.join("dance/Test");
        let parent = root.join("dance/Parent");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::write(
            skin.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=Parent\n",
        )
        .unwrap();
        std::fs::write(
            parent.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=default\n",
        )
        .unwrap();
        std::fs::copy(skin.join("Down Tap Note.png"), parent.join("unused.png")).unwrap();
        let script = skin.join("Down Tap Note.lua");
        std::fs::write(
            &script,
            "return Def.Sprite { Texture = NOTESKIN:GetPath('Down', 'Tap Note.png') }",
        )
        .unwrap();
        let layout = Layout::dance_single();
        let project = Project::open(&skin, &layout).unwrap();
        let files = crate::files::inventory(&project).unwrap();
        assert!(
            files
                .iter()
                .any(|f| f.name == "unused.png" && f.inherited && f.image)
        );
        let original = std::fs::read(&script).unwrap();
        for i in 0..3 {
            let bad = HashMap::from([(script.clone(), "return Def.Sprite { ???".into())]);
            let result =
                project.prepare_texts(&layout, false, &bad, &root.join(format!("bad-{i}")));
            assert!(
                result.is_err(),
                "invalid scripts must fail without substituting built-in actors"
            );
        }
        let good = HashMap::from([(script.clone(),"return Def.Sprite { Texture = NOTESKIN:GetPath('Down', 'Tap Note.png'), InitCommand = function(self) self:zoom(0.75) end }".into())]);
        let (mapped, ready) = project
            .prepare_texts(&layout, false, &good, &root.join("good"))
            .unwrap();
        assert!(
            ready.play.runs_lua(),
            "corrected code retries after repeated failures"
        );
        assert!(
            ready
                .play
                .element_texture_keys(0, "Tap Note")
                .iter()
                .any(|k| mapped
                    .source(k)
                    .is_some_and(|p| p == skin.join("Down Tap Note.png")))
        );
        assert!(ready.play.element_texture_keys(999, "Tap Note").is_empty());
        let export = root.join("export-with-text");
        project
            .export_texts(&export, &HashMap::new(), &good)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(export.join("NoteSkins/dance/Test/Down Tap Note.lua")).unwrap(),
            good[&script]
        );
        assert_eq!(
            std::fs::read(&script).unwrap(),
            original,
            "text edits never write the source skin"
        );
        assert!(export.join("NoteSkins/dance/Parent/unused.png").is_file());
        assert!(
            !root.join("good").exists(),
            "candidate trees are released after decoding"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
