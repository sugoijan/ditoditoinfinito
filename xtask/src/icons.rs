use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use ddi_icon::Target;
use std::{fs, path::PathBuf, process::Command};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Platform {
    Web,
    Windows,
    Linux,
    Macos,
}

impl Platform {
    fn target(self) -> Target {
        match self {
            Self::Web => Target::Web,
            Self::Windows => Target::Windows,
            Self::Linux => Target::Linux,
            Self::Macos => Target::Macos,
        }
    }
}

fn windows_ico() -> Result<Vec<u8>> {
    let mut images = Vec::new();
    for side in ddi_icon::WINDOWS_SIDES {
        images.push(
            ddi_icon::render(Target::Windows, side)
                .map_err(anyhow::Error::msg)?
                .encode_png()?,
        );
    }
    let mut ico = Vec::from([0, 0, 1, 0]);
    ico.extend((images.len() as u16).to_le_bytes());
    let mut offset = 6 + images.len() * 16;
    for (side, image) in ddi_icon::WINDOWS_SIDES.into_iter().zip(&images) {
        ico.extend([side as u8, side as u8, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((image.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += image.len();
    }
    for image in images {
        ico.extend(image);
    }
    Ok(ico)
}

pub fn generate(platform: Platform, out: Option<PathBuf>, layers: bool) -> Result<()> {
    let target = platform.target();
    if layers && target != Target::Macos {
        bail!("--layers is only used with --target macos");
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cache = root.join("target/icons").join(target.name());
    let mut files = match target {
        Target::Web => vec!["favicon.svg"],
        Target::Windows => vec!["icon.svg", "icon.ico"],
        Target::Linux => vec!["icon.svg"],
        Target::Macos => vec!["AppIcon.icns"],
    };
    if layers {
        files.extend(["foreground.svg", "background.svg"]);
    }
    // The renderer lock and both generator revisions invalidate cached pixels too.
    let lock = fs::read(root.join("Cargo.lock"))?;
    let stamp = format!(
        "{:016x}",
        ddi_icon::hash(
            ddi_icon::fingerprint()
                .to_le_bytes()
                .into_iter()
                .chain(target.name().bytes())
                .chain([u8::from(layers)])
                .chain(include_bytes!("icons.rs").iter().copied())
                .chain(lock)
        )
    );
    let cached = fs::read_to_string(cache.join(".inputs")).ok().as_deref() == Some(&stamp)
        && files.iter().all(|file| cache.join(file).is_file());
    if !cached {
        fs::create_dir_all(&cache)?;
        match target {
            Target::Web => fs::write(
                cache.join("favicon.svg"),
                ddi_icon::svg(target).map_err(anyhow::Error::msg)?,
            )?,
            Target::Linux | Target::Windows => {
                fs::write(
                    cache.join("icon.svg"),
                    ddi_icon::svg(target).map_err(anyhow::Error::msg)?,
                )?;
                if target == Target::Windows {
                    fs::write(cache.join("icon.ico"), windows_ico()?)?;
                }
            }
            Target::Macos => {
                let iconset = cache.join("AppIcon.iconset");
                fs::create_dir_all(&iconset)?;
                for side in [16, 32, 128, 256, 512] {
                    for scale in [1, 2] {
                        let suffix = if scale == 2 { "@2x" } else { "" };
                        let name = format!("icon_{side}x{side}{suffix}.png");
                        let pixels =
                            ddi_icon::render(target, side * scale).map_err(anyhow::Error::msg)?;
                        fs::write(iconset.join(name), pixels.encode_png()?)?;
                    }
                }
                let status = Command::new("iconutil")
                    .args(["-c", "icns", "-o"])
                    .arg(cache.join("AppIcon.icns"))
                    .arg(&iconset)
                    .status()
                    .context("macOS icon generation needs Apple's iconutil")?;
                if !status.success() {
                    bail!(
                        "iconutil failed; its inputs remain in {}",
                        iconset.display()
                    );
                }
                fs::remove_dir_all(iconset)?;
                if layers {
                    let (foreground, background) =
                        ddi_icon::macos_layers().map_err(anyhow::Error::msg)?;
                    fs::write(cache.join("foreground.svg"), foreground)?;
                    fs::write(cache.join("background.svg"), background)?;
                }
            }
        }
        fs::write(cache.join(".inputs"), &stamp)?;
    }
    let destination = out
        .or_else(|| {
            (target == Target::Web)
                .then(|| std::env::var_os("TRUNK_STAGING_DIR"))
                .flatten()
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| cache.clone());
    fs::create_dir_all(&destination)?;
    if fs::canonicalize(&destination)? != fs::canonicalize(&cache)? {
        for file in &files {
            fs::copy(cache.join(file), destination.join(file))?;
        }
    }
    println!(
        "{} icons{}: {}",
        target.name(),
        if cached { " (cached)" } else { "" },
        destination.display()
    );
    Ok(())
}
