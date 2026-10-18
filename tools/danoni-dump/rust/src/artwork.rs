//! Native title cards and captured-panel pixels. No browser or work execution.
use crate::{
    Error, Result, assets,
    assets::{Artifact, AssetSource, Binding, JSON_LIMIT, sha256},
};
use image::{
    RgbImage,
    imageops::{self, FilterType},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use unicode_segmentation::UnicodeSegmentation;

fn invalid(message: &str) -> Error {
    Error::new("invalid-artwork", message)
}
fn escape(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            _ => c.to_string(),
        })
        .collect()
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Banner,
    Cover,
    Pack,
}
impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Self::Banner => "banner",
            Self::Cover => "cover",
            Self::Pack => "pack",
        }
    }
    pub fn dimensions(self) -> [u32; 2] {
        match self {
            Self::Banner => [640, 200],
            Self::Cover => [768, 768],
            Self::Pack => [836, 328],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TitleRequest {
    pub title: String,
    pub artist: String,
    pub source_manifest_sha256: String,
    pub font: Binding,
    pub roles: Vec<Role>,
}
pub struct Artwork {
    pub files: Vec<Artifact>,
    pub manifest: Value,
}

/// Explicit font bytes only. No system-font discovery or external href loader.
pub const PORTABLE_LATIN: &[u8] = include_bytes!("../fonts/IosevkaTerm-Regular-latin.ttf");
pub const PORTABLE_JAPANESE: &[u8] = include_bytes!("../fonts/NotoSansCJKjp-Regular.otf");
/// Immutable assets used by the portable renderer and runtime package inventory.
pub fn portable_fonts() -> Value {
    json!({"policy":"iosevka-term-noto-cjk-jp/1", "fonts":[
        {"path":"fonts/IosevkaTerm-Regular-latin.ttf", "sha256":sha256(PORTABLE_LATIN), "license":"OFL-1.1", "family":"Iosevka Term"},
        {"path":"fonts/NotoSansCJKjp-Regular.otf", "sha256":sha256(PORTABLE_JAPANESE), "license":"OFL-1.1", "family":"Noto Sans CJK JP"}
    ], "systemFonts":false, "shaping":"usvg 0.48.1 / rustybuzz", "missingGlyphs":"reject"})
}

pub struct TitleRenderer {
    options: resvg::usvg::Options<'static>,
    family: String,
    pub font_sha256: String,
}
impl TitleRenderer {
    /// Latin face reused byte-for-byte from main, with a packaged Japanese face.
    /// usvg shapes runs and resolves missing characters within this database only.
    pub fn portable() -> Result<Self> {
        let mut renderer = Self::new(PORTABLE_LATIN)?;
        renderer
            .options
            .fontdb_mut()
            .load_font_data(PORTABLE_JAPANESE.to_vec());
        Ok(renderer)
    }
    pub fn new(font: &[u8]) -> Result<Self> {
        if font.is_empty() || font.len() > 32 * 1024 * 1024 {
            return Err(invalid("Font exceeds 32 MiB or is empty"));
        }
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().load_font_data(font.to_vec());
        let family = options
            .fontdb
            .faces()
            .next()
            .and_then(|f| f.families.first())
            .map(|(s, _)| s.clone())
            .ok_or_else(|| invalid("No usable font face"))?;
        options.font_family = family.clone();
        options.languages = vec!["ja".into(), "en".into()];
        options.image_href_resolver = resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        };
        Ok(Self {
            options,
            family,
            font_sha256: sha256(font),
        })
    }
    fn tree(&self, svg: &str) -> Result<resvg::usvg::Tree> {
        resvg::usvg::Tree::from_str(svg, &self.options).map_err(|e| invalid(&e.to_string()))
    }
    fn measure(&self, text: &str, size: u32) -> Result<f32> {
        if text.trim().is_empty() {
            return Ok(0.0);
        }
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4096\" height=\"256\"><text xml:space=\"preserve\" x=\"0\" y=\"100\" font-family=\"{}\" font-size=\"{}\">{}</text></svg>",
            escape(&self.family),
            size,
            escape(text)
        );
        let tree = self.tree(&svg)?;
        check_glyphs(tree.root())?;
        let text = tree
            .root()
            .children()
            .iter()
            .find(|n| matches!(n, resvg::usvg::Node::Text(_)))
            .ok_or_else(|| invalid("Title has no rendered glyphs"))?;
        Ok(text.abs_bounding_box().width())
    }
    fn layout(&self, title: &str, role: Role) -> Result<(Vec<String>, u32)> {
        let [width, _] = role.dimensions();
        let available = width as f32
            - if matches!(role, Role::Cover) {
                110.0
            } else {
                64.0
            };
        let maximum = match role {
            Role::Banner => 52,
            Role::Cover => 64,
            Role::Pack => 60,
        };
        for size in (16..=maximum).rev() {
            let mut lines = Vec::new();
            let mut line = String::new();
            for word in title.split_word_bounds() {
                if !line.is_empty() && self.measure(&(line.clone() + word), size)? > available {
                    lines.push(line.trim_end().to_owned());
                    line.clear();
                }
                if self.measure(word, size)? > available {
                    for glyph in word.graphemes(true) {
                        if !line.is_empty()
                            && self.measure(&(line.clone() + glyph), size)? > available
                        {
                            lines.push(line.clone());
                            line.clear();
                        }
                        line.push_str(glyph);
                    }
                } else {
                    line.push_str(word);
                }
            }
            if !line.is_empty() {
                lines.push(line.trim_end().to_owned());
            }
            if (1..=2).contains(&lines.len()) && lines.iter().all(|l| !l.trim().is_empty()) {
                return Ok((lines, size));
            }
        }
        Err(invalid(
            "Full title requires typography review; no truncation",
        ))
    }
    pub fn card(&self, title: &str, artist: &str, role: Role) -> Result<(Vec<u8>, Vec<u8>, Value)> {
        if title.trim().is_empty()
            || [title, artist]
                .iter()
                .any(|s| s.chars().count() > 300 || s.chars().any(char::is_control))
        {
            return Err(invalid("Need bounded plain title/credit text"));
        }
        let (lines, size) = self.layout(title, role)?;
        let [w, h] = role.dimensions();
        let credit_size = if matches!(role, Role::Cover) { 24 } else { 18 };
        let pad = if matches!(role, Role::Cover) { 55 } else { 32 };
        if !artist.is_empty() && self.measure(artist, credit_size)? > (w - pad * 2) as f32 {
            return Err(invalid("Full credit requires typography review"));
        }
        let baseline = (h as f64 - size as f64 * 1.2 * (lines.len() - 1) as f64) / 2.0
            + size as f64 * 0.32
            - if artist.is_empty() { 0.0 } else { 14.0 };
        let mut text = String::new();
        for (i, line) in lines.iter().enumerate() {
            text.push_str(&format!(
                "<text xml:space=\"preserve\" x=\"{}\" y=\"{}\" font-size=\"{}\">{}</text>",
                w / 2,
                baseline + i as f64 * size as f64 * 1.2,
                size,
                escape(line)
            ));
        }
        if !artist.is_empty() {
            text.push_str(&format!("<text xml:space=\"preserve\" x=\"{}\" y=\"{}\" font-size=\"{}\" opacity=\".75\">{}</text>",w/2,h-27,credit_size,escape(artist)));
        }
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" aria-label=\"{}\"><rect width=\"{w}\" height=\"{h}\" fill=\"black\"/><g fill=\"white\" font-family=\"{}\" text-anchor=\"middle\">{text}</g></svg>",
            escape(title),
            escape(&self.family)
        );
        let tree = self.tree(&svg)?;
        check_glyphs(tree.root())?;
        check_bounds(tree.root(), w, h)?;
        let mut pixmap =
            resvg::tiny_skia::Pixmap::new(w, h).ok_or_else(|| invalid("Invalid canvas size"))?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        let png = pixmap.encode_png().map_err(|e| invalid(&e.to_string()))?;
        let mut glyph_fonts = BTreeMap::<String, usize>::new();
        fn collect(
            group: &resvg::usvg::Group,
            tree: &resvg::usvg::Tree,
            fonts: &mut BTreeMap<String, usize>,
        ) {
            for node in group.children() {
                match node {
                    resvg::usvg::Node::Group(g) => collect(g, tree, fonts),
                    resvg::usvg::Node::Text(t) => {
                        for glyph in t.layouted().iter().flat_map(|s| &s.positioned_glyphs) {
                            if let Some(face) = tree.fontdb().face(glyph.font) {
                                *fonts.entry(face.families[0].0.clone()).or_default() += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        collect(tree.root(), &tree, &mut glyph_fonts);
        let proof = json!({"glyphFonts":glyph_fonts,"title":title,"artist":artist,"role":role.name(),"dimensions":[w,h],"typography":{"lines":lines,"size":size},"pngSha256":sha256(&png),"svgSha256":sha256(svg.as_bytes())});
        Ok((svg.into_bytes(), png, proof))
    }
}
fn check_glyphs(group: &resvg::usvg::Group) -> Result<()> {
    for node in group.children() {
        match node {
            resvg::usvg::Node::Group(g) => check_glyphs(g)?,
            resvg::usvg::Node::Text(t)
                if t.layouted()
                    .iter()
                    .flat_map(|s| &s.positioned_glyphs)
                    .any(|g| g.id.0 == 0 && !g.text.trim().is_empty()) =>
            {
                return Err(Error::new(
                    "missing-font-glyph",
                    "Supplied font cannot render the complete text",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}
fn check_bounds(group: &resvg::usvg::Group, width: u32, height: u32) -> Result<()> {
    for node in group.children() {
        match node {
            resvg::usvg::Node::Group(g) => check_bounds(g, width, height)?,
            resvg::usvg::Node::Text(t) => {
                let b = t.abs_bounding_box();
                if b.x() < 0.0
                    || b.y() < 0.0
                    || b.right() > width as f32
                    || b.bottom() > height as f32
                {
                    return Err(invalid("Text would be clipped by the canvas"));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
pub fn title_cards<S: AssetSource>(request: &TitleRequest, source: &S) -> Result<Artwork> {
    let mut inputs = assets::Inputs::new(source);
    let font = inputs.checked(&request.font, 32 * 1024 * 1024)?;
    let renderer = TitleRenderer::new(&font)?;
    render_cards(request, &renderer, false)
}
/// Portable generated artwork; no host fonts or font filesystem paths required.
pub fn portable_title_cards(
    title: &str,
    artist: &str,
    source_hash: &str,
    roles: Vec<Role>,
) -> Result<Artwork> {
    let request = TitleRequest {
        title: title.into(),
        artist: artist.into(),
        source_manifest_sha256: source_hash.into(),
        font: Binding {
            namespace: "portable".into(),
            path: "IosevkaTerm-Regular-latin.ttf".into(),
            sha256: sha256(PORTABLE_LATIN),
        },
        roles,
    };
    render_cards(&request, &TitleRenderer::portable()?, true)
}
fn render_cards(
    request: &TitleRequest,
    renderer: &TitleRenderer,
    portable: bool,
) -> Result<Artwork> {
    if request.source_manifest_sha256.len() != 64
        || !request
            .source_manifest_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(invalid("Need lowercase source manifest SHA-256"));
    }
    if request.roles.is_empty()
        || request.roles.len() > 3
        || request
            .roles
            .iter()
            .map(|r| r.name())
            .collect::<BTreeSet<_>>()
            .len()
            != request.roles.len()
    {
        return Err(invalid("Select unique artwork roles"));
    }
    let mut files = Vec::new();
    let mut records = Vec::new();
    for role in &request.roles {
        let (svg, png, mut proof) = renderer.card(&request.title, &request.artist, *role)?;
        let name = role.name();
        proof["svg"] = json!(format!("{name}.svg"));
        proof["png"] = json!(format!("{name}.png"));
        files.push(Artifact {
            path: format!("{name}.svg"),
            bytes: Arc::new(svg),
        });
        files.push(Artifact {
            path: format!("{name}.png"),
            bytes: Arc::new(png),
        });
        records.push(proof);
    }
    let manifest = json!({"schema":"ddi-artwork-study/experimental-1","template":"plain-title-rust/1","sourceManifestSha256":request.source_manifest_sha256,"fontSha256":renderer.font_sha256,"fontFamily":renderer.family,"renderer":"resvg 0.48.1 / tiny-skia 0.12.0","fontDistributed":portable,"portableFonts":if portable {portable_fonts()}else{Value::Null},"originalAssetsUsed":false,"networkRequests":0,"records":records});
    Ok(Artwork { files, manifest })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelRequest {
    pub snapshot: Binding,
    pub run: Binding,
    pub image: Binding,
    pub frame: u64,
    pub clip: String,
    pub expected_clip_frame: Option<u64>,
}
fn native_string(value: &Value) -> Result<String> {
    if value["type"] != "string" || value["truncated"] == true {
        return Err(invalid("Need complete native name"));
    }
    let units = value["utf16"]
        .as_array()
        .ok_or_else(|| invalid("Missing UTF-16 name"))?
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| invalid("Invalid UTF-16 unit"))
        })
        .collect::<Result<Vec<_>>>()?;
    String::from_utf16(&units).map_err(|_| invalid("Invalid native Unicode name"))
}
fn ppm(bytes: &[u8]) -> Result<RgbImage> {
    let at = bytes
        .windows(5)
        .position(|v| v == b"\n255\n")
        .ok_or_else(|| invalid("Need worker P6 image"))?;
    let header = std::str::from_utf8(&bytes[..at])
        .map_err(|_| invalid("Invalid PPM header"))?
        .split_whitespace()
        .collect::<Vec<_>>();
    if header.len() != 3 || header[0] != "P6" {
        return Err(invalid("Invalid PPM header"));
    }
    let parse = |s: &str| {
        s.parse::<u32>()
            .map_err(|_| invalid("Invalid PPM dimension"))
    };
    let (w, h) = (parse(header[1])?, parse(header[2])?);
    if !(1..=1280).contains(&w)
        || !(1..=960).contains(&h)
        || bytes.len() - at - 5 != w as usize * h as usize * 3
    {
        return Err(invalid("Invalid/oversized captured pixels"));
    }
    RgbImage::from_raw(w, h, bytes[at + 5..].to_vec()).ok_or_else(|| invalid("Invalid RGB image"))
}
fn crop_rect(state: &Value, name: &str, width: u32, height: u32) -> Result<([u32; 4], Value)> {
    if state["incomplete"] != false || state["avm1Halted"] != false {
        return Err(invalid("Incomplete native state"));
    }
    let displays = state["displayTree"]
        .as_array()
        .ok_or_else(|| invalid("Missing display tree"))?;
    if displays.len() > 4096 {
        return Err(invalid("Display tree exceeds limit"));
    }
    let mut indexed = BTreeMap::new();
    for d in displays {
        let id = d["id"]
            .as_u64()
            .ok_or_else(|| invalid("Invalid display id"))?;
        if indexed.insert(id, d).is_some() {
            return Err(invalid("Duplicate display id"));
        }
    }
    let mut found = Vec::new();
    for d in displays {
        if d["kind"] != "movie-clip" || d["name"].is_null() || native_string(&d["name"])? != name {
            continue;
        }
        let mut current = d;
        let mut seen = BTreeSet::new();
        let mut visible = true;
        loop {
            let id = current["id"].as_u64().unwrap();
            if !seen.insert(id) {
                return Err(invalid("Cyclic display ancestry"));
            }
            if current["visible"] != true {
                visible = false;
                break;
            }
            if current["parent"].is_null() {
                break;
            }
            let parent = current["parent"]
                .as_u64()
                .ok_or_else(|| invalid("Invalid display parent"))?;
            current = indexed
                .get(&parent)
                .ok_or_else(|| invalid("Incomplete display ancestry"))?;
        }
        if visible {
            found.push(d);
        }
    }
    if found.len() != 1 {
        return Err(invalid("Need one visible named title clip"));
    }
    let display = found[0];
    let bounds: Vec<f64> = serde_json::from_value(display["stageBoundsPixels"].clone())
        .map_err(|_| invalid("Invalid bounds"))?;
    let matrix: Vec<f64> = serde_json::from_value(state["stageToViewport"].clone())
        .map_err(|_| invalid("Invalid transform"))?;
    if bounds.len() != 4
        || matrix.len() != 6
        || bounds.iter().chain(&matrix).any(|v| !v.is_finite())
        || bounds[0] >= bounds[2]
        || bounds[1] >= bounds[3]
    {
        return Err(invalid("Invalid native geometry"));
    }
    let mut points = Vec::new();
    for x in [bounds[0], bounds[2]] {
        for y in [bounds[1], bounds[3]] {
            points.push((
                matrix[0] * x + matrix[2] * y + matrix[4],
                matrix[1] * x + matrix[3] * y + matrix[5],
            ));
        }
    }
    if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Err(invalid("Overflowed native transform"));
    }
    let left = points
        .iter()
        .map(|p| p.0)
        .fold(f64::INFINITY, f64::min)
        .floor()
        - 1.0;
    let right = points
        .iter()
        .map(|p| p.0)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        + 1.0;
    let top = points
        .iter()
        .map(|p| p.1)
        .fold(f64::INFINITY, f64::min)
        .floor()
        - 1.0;
    let bottom = points
        .iter()
        .map(|p| p.1)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        + 1.0;
    if !(0.0 <= left
        && left < right
        && right <= width as f64
        && 0.0 <= top
        && top < bottom
        && bottom <= height as f64)
    {
        return Err(invalid("Title panel extends outside capture"));
    }
    Ok((
        [
            left as u32,
            top as u32,
            (right - left) as u32,
            (bottom - top) as u32,
        ],
        display.clone(),
    ))
}
fn png(image: &RgbImage) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| invalid(&e.to_string()))?;
    Ok(out.into_inner())
}
pub fn title_panel<S: AssetSource>(request: &PanelRequest, source: &S) -> Result<Artwork> {
    if !(1..=1200).contains(&request.frame)
        || request.clip.is_empty()
        || request.clip.chars().count() > 300
        || request.clip.chars().any(char::is_control)
    {
        return Err(invalid("Invalid capture selector"));
    }
    let mut inputs = assets::Inputs::new(source);
    let snapshot_raw = inputs.checked(&request.snapshot, 16 * 1024 * 1024)?;
    let run_raw = inputs.checked(&request.run, JSON_LIMIT)?;
    let image_raw = inputs.checked(&request.image, 16 * 1024 * 1024)?;
    let snapshot: Value = assets::json(&snapshot_raw)?;
    let run: Value = assets::json(&run_raw)?;
    let is_hash = |v: &Value| {
        v.as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    };
    if snapshot["schema"] != "ddi-ruffle-probe/experimental-1"
        || run["schema"] != "ddi-ruffle-run/experimental-1"
        || !is_hash(&snapshot["source"]["movieSha256"])
        || !is_hash(&run["workerSha256"])
        || !is_hash(&run["sandboxSha256"])
        || !is_hash(&run["configSha256"])
        || !run["startedAt"].as_str().is_some_and(|s| !s.is_empty())
        || !snapshot["source"]["config"]["sourceUrl"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    {
        return Err(invalid("Missing captured source/worker/sandbox provenance"));
    }
    if run["exitCode"] != 0
        || !run["termination"].is_null()
        || run["inputsUnchanged"] != true
        || snapshot["rendered"] != true
        || snapshot["avm1Halted"] != false
        || snapshot["ruffleRevision"] != "89a7049965f3090e706265670df309a5128233ac"
    {
        return Err(invalid("Need successful reviewed restricted rendering"));
    }
    for field in [
        "truncated",
        "runtimeErrors",
        "droppedEntries",
        "truncatedDetails",
    ] {
        if snapshot["audit"][field] != 0 && snapshot["audit"][field] != false {
            return Err(invalid("Truncated/failed execution audit"));
        }
    }
    let resources = snapshot["source"]["config"]["resources"]
        .as_array()
        .map(|rs| {
            rs.iter()
                .filter_map(|r| r["url"].as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    for entry in snapshot["audit"]["entries"]
        .as_array()
        .ok_or_else(|| invalid("Missing audit entries"))?
    {
        if entry["truncated"] == true
            || entry["kind"] != "runtime-warning"
                && !(entry["kind"] == "cache-hit"
                    && entry["detail"]
                        .as_str()
                        .is_some_and(|s| resources.contains(s)))
        {
            return Err(invalid("Unexpected runtime audit entry"));
        }
    }
    let captures = snapshot["captures"]
        .as_array()
        .ok_or_else(|| invalid("Missing captures"))?
        .iter()
        .filter(|c| c["frame"] == request.frame)
        .collect::<Vec<_>>();
    if captures.len() != 1 {
        return Err(invalid("Missing/ambiguous capture frame"));
    }
    let pixels = ppm(&image_raw)?;
    let (crop, display) = crop_rect(
        &captures[0]["state"],
        &request.clip,
        pixels.width(),
        pixels.height(),
    )?;
    if request
        .expected_clip_frame
        .is_some_and(|n| display["movieFrame"] != n)
    {
        return Err(invalid("Selected clip frame differs from recipe"));
    }
    let panel = imageops::crop_imm(&pixels, crop[0], crop[1], crop[2], crop[3]).to_image();
    let scale = (640.0 / crop[2] as f64).min(200.0 / crop[3] as f64);
    let w = (crop[2] as f64 * scale).round().clamp(1.0, 640.0) as u32;
    let h = (crop[3] as f64 * scale).round().clamp(1.0, 200.0) as u32;
    let fitted = imageops::resize(&panel, w, h, FilterType::Lanczos3);
    let mut banner = RgbImage::new(640, 200);
    imageops::replace(
        &mut banner,
        &fitted,
        i64::from((640 - w) / 2),
        i64::from((200 - h) / 2),
    );
    let panel_bytes = png(&panel)?;
    let banner_bytes = png(&banner)?;
    let manifest = json!({"schema":"ddi-native-title-panel/2","sourceSwfSha256":snapshot["source"]["movieSha256"],"sourceUrl":snapshot["source"]["config"]["sourceUrl"],"pageUrl":snapshot["source"]["config"]["pageUrl"],"capturedAt":run["startedAt"],"workerSha256":run["workerSha256"],"ruffle":snapshot["build"],"inputs":request,"captureFrame":request.frame,"clipName":request.clip,"clipMovieFrame":display["movieFrame"],"stageBoundsPixels":display["stageBoundsPixels"],"stageToViewport":captures[0]["state"]["stageToViewport"],"cropPixels":crop,"bannerDimensions":[640,200],"background":"black","titleReplaced":false,"transformation":{"implementation":"image 0.25.10","filter":"Lanczos3","fitDimensions":[w,h],"matteOrigin":[(640-w)/2,(200-h)/2]},"files":{"panel.png":sha256(&panel_bytes),"banner.png":sha256(&banner_bytes)},"scope":"Lossless captured title crop; native typography retained; Rust resize differs from FFmpeg rasterization"});
    Ok(Artwork {
        files: vec![
            Artifact {
                path: "panel.png".into(),
                bytes: Arc::new(panel_bytes),
            },
            Artifact {
                path: "banner.png".into(),
                bytes: Arc::new(banner_bytes),
            },
        ],
        manifest,
    })
}
