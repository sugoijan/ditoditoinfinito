//! The approved app icon: one SVG artwork, frozen color field and target
//! framing profiles. The lightweight composer is used by native build
//! scripts; `raster` adds the shared resvg renderer for packaging and xtask.

use base64::Engine as _;
use serde::Deserialize;
use std::collections::BTreeMap;

const ART: &str = include_str!("../../../assets/icon/arrow.svg");
const FIELD: &[u8] = include_bytes!("../../../assets/icon/color.png");
const PROFILES: &str = include_str!("../../../assets/icon/profiles.json");

/// Cargo build scripts watch exactly the canonical inputs.
pub const INPUTS: [&str; 3] = [
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/icon/arrow.svg"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/icon/color.png"),
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/icon/profiles.json"
    ),
];

/// The sides of the Windows executable's icon group.
pub const WINDOWS_SIDES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Web,
    Windows,
    Linux,
    Macos,
}

impl Target {
    pub fn name(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Macos => "macos",
        }
    }

    /// The inverse of [`Target::name`].
    pub fn parse(name: &str) -> Result<Target> {
        Ok(match name {
            "web" => Self::Web,
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "macos" => Self::Macos,
            other => {
                return Err(format!(
                    "no icon target {other:?}: web, windows, linux or macos"
                ));
            }
        })
    }

    fn profile(self) -> &'static str {
        match self {
            Self::Web => "favicon",
            Self::Macos => "macos",
            Self::Windows | Self::Linux => "windows",
        }
    }
}

#[derive(Deserialize)]
struct Profiles {
    version: u32,
    targets: BTreeMap<String, Profile>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    size: f64,
    x: f64,
    y: f64,
    radius: f64,
    background: String,
    plate: bool,
    shadow: f64,
    shadow_radius: f64,
    dark_gap: bool,
    gap_color: String,
    gap_opacity: f64,
    border: bool,
    border_ratio: f64,
    border_opacity: f64,
    charge_color: String,
}

type Result<T> = std::result::Result<T, String>;

fn profile(target: Target) -> Result<Profile> {
    let mut profiles: Profiles = serde_json::from_str(PROFILES).map_err(|e| e.to_string())?;
    if profiles.version != 1 {
        return Err("unsupported icon profile version".into());
    }
    profiles
        .targets
        .remove(target.profile())
        .ok_or_else(|| format!("missing {} icon profile", target.profile()))
}

fn compose(target: Target, field: &[u8], foreground: bool) -> Result<String> {
    compose_profile(target, field, foreground, profile(target)?)
}

fn compose_profile(target: Target, field: &[u8], foreground: bool, p: Profile) -> Result<String> {
    let doc = roxmltree::Document::parse(ART).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let metric = |name| -> Result<f64> {
        root.attribute(name)
            .ok_or_else(|| format!("icon artwork has no {name}"))?
            .parse::<f64>()
            .map_err(|e| e.to_string())
    };
    let span = metric("data-band-span")?
        + if p.border {
            metric("data-border-unit-span")? * p.border_ratio
        } else {
            0.0
        };
    let border_width = metric("data-border-unit-width")? * p.border_ratio;
    let defs = root
        .children()
        .find(|n| n.has_tag_name("defs"))
        .ok_or("icon artwork has no definitions")?;
    let arrow = root
        .children()
        .find(|n| n.attribute("id") == Some("icon-arrow"))
        .ok_or("icon artwork has no drawing")?;
    let mut drawing = String::new();
    for node in arrow.children().filter(|n| n.is_element()) {
        match node.attribute("data-role") {
            Some("shadow") if p.shadow > 0.0 => {
                drawing.push_str(&format!(
                    "<use href=\"#icon-shadow\" opacity=\"{}\"/>",
                    p.shadow
                ));
            }
            Some("outer-border") if p.border => drawing.push_str(&format!(
                "<use href=\"#icon-border\" mask=\"url(#icon-shadow-outside)\" color=\"{}\" stroke-width=\"{border_width}\" opacity=\"{}\"/>",
                p.gap_color, p.border_opacity
            )),
            Some("inner-edge") if p.dark_gap => {
                let mask = node.attribute("mask").map(|m| format!(" mask=\"{m}\"")).unwrap_or_default();
                drawing.push_str(&format!(
                    "<use href=\"#icon-silhouette\" color=\"{}\" opacity=\"{}\"{mask}/>",
                    p.gap_color, p.gap_opacity
                ));
            },
            Some("charge") => drawing.push_str(&format!(
                "<use href=\"#tap-arrow-charge\" color=\"{}\"/>", p.charge_color
            )),
            Some("shadow" | "outer-border" | "inner-edge") => {},
            _ => drawing.push_str(&ART[node.range()]),
        }
    }
    let mut definitions = ART[defs.range()].to_owned();
    if let Some(blur) = defs
        .descendants()
        .find(|n| n.has_tag_name("feGaussianBlur"))
    {
        definitions = definitions.replace(
            &ART[blur.range()],
            &format!("<feGaussianBlur stdDeviation=\"{}\"/>", p.shadow_radius),
        );
    }
    drawing = drawing.replace(
        "href=\"color.png\"",
        &format!(
            "href=\"data:image/png;base64,{}\"",
            base64::engine::general_purpose::STANDARD.encode(field)
        ),
    );
    let scale = 1024.0 * p.size / (100.0 * span);
    let art = format!(
        "<g transform=\"translate({} {}) scale({scale})\"><g transform=\"{}\">{drawing}</g></g>",
        512.0 + p.x * 10.24,
        512.0 + p.y * 10.24,
        arrow.attribute("transform").unwrap_or_default(),
    );
    let layers = if p.plate && !foreground {
        let (margin, side) = if target == Target::Macos {
            (100, 824)
        } else {
            (0, 1024)
        };
        let bounds = format!(
            "x=\"{margin}\" y=\"{margin}\" width=\"{side}\" height=\"{side}\" rx=\"{}\"",
            f64::from(side) * p.radius / 100.0
        );
        format!(
            "<defs><clipPath id=\"platform-tile\"><rect {bounds}/></clipPath></defs>\
             <rect {bounds} fill=\"{}\"/>\
             <g clip-path=\"url(#platform-tile)\"><g transform=\"translate({margin} {margin}) scale({})\">{art}</g></g>",
            p.background,
            f64::from(side) / 1024.0
        )
    } else {
        art
    };
    let side = if target == Target::Web { 64 } else { 1024 };
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{side}\" height=\"{side}\" viewBox=\"0 0 1024 1024\">\
         <title>Dito Dito Infinito</title>{}{layers}</svg>\n",
        definitions
    ))
}

/// The SVG embedded by the Windows/Linux desktop's build script.
pub fn desktop_svg() -> Result<String> {
    compose(Target::Windows, FIELD, false)
}

/// Inputs and compositor revision for the build-output cache (not a security hash).
pub fn fingerprint() -> u64 {
    hash(
        [
            ART.as_bytes(),
            FIELD,
            PROFILES.as_bytes(),
            include_bytes!("lib.rs"),
        ]
        .into_iter()
        .flatten()
        .copied(),
    )
}

fn hash(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(feature = "raster")]
pub fn svg(target: Target) -> Result<String> {
    if target != Target::Web {
        return compose(target, FIELD, false);
    }
    use resvg::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
    let original = Pixmap::decode_png(FIELD).map_err(|e| e.to_string())?;
    let mut small = Pixmap::new(256, 256).ok_or("empty icon texture")?;
    small.draw_pixmap(
        0,
        0,
        original.as_ref(),
        &PixmapPaint {
            quality: FilterQuality::Bicubic,
            ..PixmapPaint::default()
        },
        Transform::from_scale(
            256.0 / original.width() as f32,
            256.0 / original.height() as f32,
        ),
        None,
    );
    compose(
        target,
        &small.encode_png().map_err(|e| e.to_string())?,
        false,
    )
}

#[cfg(feature = "raster")]
pub fn render(target: Target, side: u32) -> Result<resvg::tiny_skia::Pixmap> {
    render_svg(&svg(target)?, side)
}

#[cfg(feature = "raster")]
fn render_svg(text: &str, side: u32) -> Result<resvg::tiny_skia::Pixmap> {
    let tree = resvg::usvg::Tree::from_str(text, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(side, side).ok_or("empty icon size")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            side as f32 / tree.size().width(),
            side as f32 / tree.size().height(),
        ),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// Optional authoring outputs, separate from the legacy ICNS composition.
#[cfg(feature = "raster")]
pub fn macos_layers() -> Result<(String, String)> {
    let p = profile(Target::Macos)?;
    Ok((
        compose(Target::Macos, FIELD, true)?,
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1024\" height=\"1024\"><rect width=\"1024\" height=\"1024\" fill=\"{}\"/></svg>\n",
            p.background
        ),
    ))
}

#[cfg(all(test, feature = "raster"))]
mod tests {
    use super::*;

    #[test]
    fn the_canonical_art_draws_on_every_target() {
        for target in [Target::Web, Target::Windows, Target::Linux, Target::Macos] {
            let icon = render(target, 64).unwrap();
            assert_eq!(icon.pixel(1, 1).unwrap().alpha(), 0, "{target:?}");
            assert!(
                icon.pixels().iter().any(|p| {
                    let p = p.demultiply();
                    p.alpha() == 255 && p.red().max(p.blue()) - p.green().min(p.blue()) > 80
                }),
                "{target:?}"
            );
        }
        assert_eq!(
            render(Target::Windows, 64)
                .unwrap()
                .pixel(46, 32)
                .unwrap()
                .alpha(),
            0
        );
    }

    #[test]
    fn the_desktop_build_and_packaging_share_the_svg() {
        assert_eq!(desktop_svg().unwrap(), svg(Target::Windows).unwrap());
        assert_eq!(svg(Target::Windows).unwrap(), svg(Target::Linux).unwrap());
    }

    #[test]
    fn outer_opacity_leaves_the_inner_edge_and_color_field_unchanged() {
        let mut p = profile(Target::Windows).unwrap();
        p.shadow = 0.0;
        p.border_opacity = 1.0;
        let opaque = render_svg(
            &compose_profile(Target::Windows, FIELD, false, p.clone()).unwrap(),
            64,
        )
        .unwrap();
        p.border_opacity = 0.25;
        let faint = render_svg(
            &compose_profile(Target::Windows, FIELD, false, p).unwrap(),
            64,
        )
        .unwrap();
        assert_eq!(opaque.pixel(32, 32), faint.pixel(32, 32));
        assert_eq!(opaque.pixel(54, 40), faint.pixel(54, 40));
        assert_eq!(faint.pixel(46, 32).unwrap().alpha(), 0);
        let changed: Vec<_> = opaque
            .pixels()
            .iter()
            .zip(faint.pixels())
            .filter(|(a, b)| a != b)
            .collect();
        assert!(!changed.is_empty());
        assert!(changed.iter().all(|(a, b)| a.alpha() > b.alpha()));
    }
}
