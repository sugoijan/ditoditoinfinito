//! The complete fallback-chain inventory, independent of the current preview layout.
use crate::project::Project;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct File {
    pub path: PathBuf,
    pub name: String,
    pub owner: String,
    pub inherited: bool,
    pub image: bool,
    pub text: bool,
}

pub fn image(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "bmp"
                | "tga"
                | "dds"
                | "webp"
                | "tif"
                | "tiff"
                | "svg"
        )
    })
}

pub fn inventory(project: &Project) -> Result<Vec<File>, String> {
    let mut out = Vec::new();
    for index in &project.chain {
        let skin = project.skins.get(*index).ok_or("Missing fallback skin")?;
        let walk = ddi_desktop::library::files::walk(&skin.dir);
        if !walk.unread.is_empty() {
            return Err(format!("Could not list {}: {:?}", skin.name, walk.unread));
        }
        for file in walk.listed {
            if out.len() >= 4096 {
                return Err("Project exceeds 4096 files".into());
            }
            let name = file.path.as_str().to_string();
            let path = skin.dir.join(&name);
            let is_image = image(&path);
            let text = ddi_noteskin::import::kept_text(&name)
                || name.to_ascii_lowercase().ends_with(".svg")
                || matches!(
                    path.extension().and_then(|s| s.to_str()),
                    Some("md" | "toml" | "json")
                )
                || path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("LICENSE"));
            out.push(File {
                path,
                name,
                owner: skin.name.clone(),
                inherited: *index != 0,
                image: is_image,
                text,
            });
        }
    }
    out.sort_by(|a, b| (a.inherited, &a.owner, &a.name).cmp(&(b.inherited, &b.owner, &b.name)));
    Ok(out)
}

pub fn read_text(path: &Path) -> Result<String, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 1 << 20 {
        return Err("Text files must fit within 1 MiB".into());
    }
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}
