use anyhow::{Context, Result, bail};
use ddi_icon::{Target, Variant};
use std::{fs, path::PathBuf, process::Command};

pub fn generate(
    target: Target,
    variant: Option<Variant>,
    out: Option<PathBuf>,
    layers: bool,
) -> Result<()> {
    if layers && target != Target::Macos {
        bail!("--layers is only used with --target macos");
    }
    let variant = match variant {
        Some(v) => v,
        None => Variant::select(
            std::env::var("DDI_ICON_VARIANT").ok().as_deref(),
            std::env::var("TRUNK_PROFILE").ok().as_deref(),
        )
        .map_err(anyhow::Error::msg)?,
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cache = root
        .join("target/icons")
        .join(target.name())
        .join(variant.name());
    let mut files = match target {
        Target::Web => vec!["favicon.svg"],
        Target::Windows | Target::Linux => vec!["icon.svg"],
        Target::Macos => vec!["AppIcon.icns"],
    };
    if layers {
        files.extend(["foreground.svg", "background.svg"]);
    }
    // The renderer lock and both generator revisions invalidate cached pixels too.
    let mut stamp = ddi_library::pack::Fnv1a64::new();
    stamp.write(&ddi_icon::fingerprint(variant).to_le_bytes());
    stamp.write(variant.name().as_bytes());
    stamp.write(target.name().as_bytes());
    stamp.write(&[u8::from(layers)]);
    stamp.write(include_bytes!("icons.rs"));
    stamp.write(&fs::read(root.join("Cargo.lock"))?);
    let stamp = format!("{:016x}", stamp.finish());
    let cached = fs::read_to_string(cache.join(".inputs")).ok().as_deref() == Some(&stamp)
        && files.iter().all(|file| cache.join(file).is_file());
    if !cached {
        fs::create_dir_all(&cache)?;
        match target {
            Target::Web => fs::write(
                cache.join("favicon.svg"),
                ddi_icon::svg(target, variant).map_err(anyhow::Error::msg)?,
            )?,
            Target::Linux | Target::Windows => fs::write(
                cache.join("icon.svg"),
                ddi_icon::svg(target, variant).map_err(anyhow::Error::msg)?,
            )?,
            Target::Macos => {
                let iconset = cache.join("AppIcon.iconset");
                fs::create_dir_all(&iconset)?;
                for side in [16, 32, 128, 256, 512] {
                    for scale in [1, 2] {
                        let suffix = if scale == 2 { "@2x" } else { "" };
                        let name = format!("icon_{side}x{side}{suffix}.png");
                        let pixels = ddi_icon::render(target, variant, side * scale)
                            .map_err(anyhow::Error::msg)?;
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
                        ddi_icon::macos_layers(variant).map_err(anyhow::Error::msg)?;
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
        "{} {} icons{}: {}",
        target.name(),
        variant.name(),
        if cached { " (cached)" } else { "" },
        destination.display()
    );
    Ok(())
}
