use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Deserialize;

mod boot_check;
mod credits;
mod dito;
mod noteskins;
mod rust_credits;
mod songs;

#[derive(Parser)]
#[command(about = "Build and maintenance tasks for the DitoDitoInfinito workspace")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Regenerate the SEO metadata fragments and sitemap from site/seo/metadata.toml.
    #[command(name = "regen-seo")]
    RegenSeo,
    /// Generate target/songs/index.json from assets/songs/*/ (chart + PROVENANCE.toml).
    #[command(name = "gen-songs")]
    GenSongs {
        /// Fail instead of writing when the output would change (CI check).
        #[arg(long)]
        check: bool,
    },
    /// Generate the SVG art of the bundled skin Dito (the built-in arrows
    /// as a skin) under assets/noteskins/dance/Dito*/.
    #[command(name = "gen-dito")]
    GenDito {
        /// Fail instead of writing when a file would change (CI check).
        #[arg(long)]
        check: bool,
    },
    /// Check the bundled noteskins (assets/noteskins/<game>/<name>/ with a
    /// PROVENANCE.toml) and generate target/noteskins/index.json.
    #[command(name = "gen-noteskins")]
    GenNoteskins {
        /// Fail instead of writing when the output would change (CI check).
        #[arg(long)]
        check: bool,
    },
    /// Generate credits/CREDITS.md and target/credits/credits.json from the song
    /// provenance records and REUSE.toml.
    #[command(name = "gen-credits")]
    GenCredits {
        /// Fail instead of writing when CREDITS.md would change (CI check).
        #[arg(long)]
        check: bool,
    },
    /// Copy the video decoder (target/video/) into the site being built,
    /// building it first (`video/build.sh`, as `just video`) when it is
    /// missing or older than its sources; DDI_NO_VIDEO=1 builds the site
    /// without it. Trunk runs this after a build, with TRUNK_STAGING_DIR
    /// set.
    #[command(name = "stage-video")]
    StageVideo {
        /// The site's folder (default: $TRUNK_STAGING_DIR).
        #[arg(long)]
        dest: Option<PathBuf>,
    },
    /// Name the Opus worker's files (Trunk writes `opus-worker.js` and
    /// `opus-worker_bg.wasm`) by their content's hash and tell the page
    /// in index.html, as Trunk names the app's: a page never mixes two
    /// builds' files. Trunk runs this after a build.
    #[command(name = "stage-opus")]
    StageOpus {
        /// The site's folder (default: $TRUNK_STAGING_DIR).
        #[arg(long)]
        dest: Option<PathBuf>,
    },
    /// Check the built index.html draws its loader from its own first
    /// bytes: within the first round trip compressed, with nothing the
    /// first paint waits for fetched (`xtask/src/boot_check.rs`). Trunk
    /// runs this after a build.
    #[command(name = "check-boot")]
    CheckBoot {
        /// The site's folder (default: $TRUNK_STAGING_DIR).
        #[arg(long)]
        dest: Option<PathBuf>,
    },
}

struct SeoPaths {
    metadata: PathBuf,
    generated_dir: PathBuf,
    head: PathBuf,
    body: PathBuf,
    sitemap: PathBuf,
    theme: PathBuf,
    theme_css: PathBuf,
}

fn seo_paths() -> SeoPaths {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate lives inside the workspace root")
        .to_path_buf();
    let generated_dir = repo_root.join("target/seo");
    SeoPaths {
        metadata: repo_root.join("site/seo/metadata.toml"),
        head: generated_dir.join("seo-head.html"),
        body: generated_dir.join("seo-body.html"),
        sitemap: generated_dir.join("sitemap.xml"),
        theme: repo_root.join("crates/theme/src/infinito.toml"),
        theme_css: generated_dir.join("theme.css"),
        generated_dir,
    }
}

#[derive(Debug, Deserialize)]
struct SeoManifest {
    default_locale: String,
    site_name: String,
    alternate_name: Option<String>,
    /// The theme's ground, the browser's color for the page (from the
    /// theme, not the metadata).
    #[serde(skip)]
    theme_color: String,
    canonical_url: String,
    favicon_href: String,
    apple_touch_icon_href: Option<String>,
    social_image_url: Option<String>,
    social_image_alt: Option<String>,
    social_image_type: Option<String>,
    social_image_width: Option<u32>,
    social_image_height: Option<u32>,
    robots: String,
    application_category: String,
    operating_system: String,
    genres: Vec<String>,
    play_modes: Vec<String>,
    twitter_card: Option<String>,
    twitter_site: Option<String>,
    is_accessible_for_free: Option<bool>,
    offers_price: Option<String>,
    offers_price_currency: Option<String>,
    locales: BTreeMap<String, SeoLocale>,
}

#[derive(Debug, Deserialize)]
struct SeoLocale {
    lang: String,
    title: String,
    description: String,
    structured_data_description: String,
    accessible_name: String,
    accessible_summary: String,
    noscript_title: String,
    noscript: String,
    og_locale: Option<String>,
}

impl SeoManifest {
    fn default_locale_entry(&self) -> Result<(&str, &SeoLocale)> {
        let locale_key = self.default_locale.as_str();
        let locale = self
            .locales
            .get(locale_key)
            .with_context(|| format!("missing [locales.{locale_key}] entry in SEO manifest"))?;
        Ok((locale_key, locale))
    }
}

const GENERATED_BANNER: &str = "<!-- Generated by `cargo run -p xtask -- regen-seo`. Edit `site/seo/metadata.toml` instead. -->";

fn regen_seo(paths: &SeoPaths) -> Result<()> {
    fs::create_dir_all(&paths.generated_dir)
        .with_context(|| format!("failed to create {}", paths.generated_dir.display()))?;

    let manifest_text = fs::read_to_string(&paths.metadata)
        .with_context(|| format!("failed to read {}", paths.metadata.display()))?;
    let mut manifest: SeoManifest = toml::from_str(&manifest_text)
        .with_context(|| format!("failed to parse {}", paths.metadata.display()))?;
    let theme_text = fs::read_to_string(&paths.theme)
        .with_context(|| format!("failed to read {}", paths.theme.display()))?;
    let colors = theme_colors(&theme_text)
        .with_context(|| format!("failed to read the colors of {}", paths.theme.display()))?;
    manifest.theme_color = theme_color(&colors, "Ground")?.to_string();
    let (_locale_key, locale) = manifest.default_locale_entry()?;

    fs::write(&paths.head, render_seo_head(&manifest, locale)?)
        .with_context(|| format!("failed to write {}", paths.head.display()))?;
    fs::write(&paths.body, render_seo_body(locale))
        .with_context(|| format!("failed to write {}", paths.body.display()))?;
    fs::write(&paths.sitemap, render_seo_sitemap(&manifest))
        .with_context(|| format!("failed to write {}", paths.sitemap.display()))?;
    fs::write(&paths.theme_css, render_theme_css(&colors)?)
        .with_context(|| format!("failed to write {}", paths.theme_css.display()))?;
    Ok(())
}

/// The page's colors before the game draws (the loader, the pages drawn
/// in the DOM), as CSS variables, by the theme's names for them: the
/// page follows the theme alone, whatever the system's color scheme.
const PAGE_COLORS: [(&str, &str); 5] = [
    ("ground", "Ground"),
    ("text", "Text"),
    ("muted", "Muted"),
    ("accent", "Accent"),
    ("line", "Line2"),
];

fn theme_colors(theme: &str) -> Result<BTreeMap<String, String>> {
    #[derive(Deserialize)]
    struct Theme {
        colors: BTreeMap<String, String>,
    }
    Ok(toml::from_str::<Theme>(theme)?.colors)
}

fn theme_color<'a>(colors: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    colors
        .get(name)
        .map(String::as_str)
        .with_context(|| format!("the theme has no color {name:?}"))
}

fn render_theme_css(colors: &BTreeMap<String, String>) -> Result<String> {
    let mut css = String::from(
        "/* Generated by `cargo run -p xtask -- regen-seo` from crates/theme/src/infinito.toml. */\n:root {\n",
    );
    for (var, name) in PAGE_COLORS {
        writeln!(css, "  --{var}: {};", theme_color(colors, name)?)?;
    }
    css.push_str("}\n");
    Ok(css)
}

fn icon_mime_type(href: &str) -> &'static str {
    let path = href.split(['?', '#']).next().unwrap_or(href);
    match path.rsplit('.').next() {
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

fn render_seo_head(manifest: &SeoManifest, locale: &SeoLocale) -> Result<String> {
    let mut main_entity = serde_json::json!({
        "@type": "VideoGame",
        "name": &manifest.site_name,
        "url": &manifest.canonical_url,
        "description": &locale.structured_data_description,
        "genre": &manifest.genres,
        "applicationCategory": &manifest.application_category,
        "operatingSystem": &manifest.operating_system,
    });
    if let Some(image) = &manifest.social_image_url {
        main_entity["image"] = serde_json::json!(image);
        main_entity["screenshot"] = serde_json::json!(image);
    }
    if let Some(alternate_name) = &manifest.alternate_name {
        main_entity["alternateName"] = serde_json::json!(alternate_name);
    }
    if !manifest.play_modes.is_empty() {
        main_entity["playMode"] = serde_json::json!(&manifest.play_modes);
    }
    if let Some(free) = manifest.is_accessible_for_free {
        main_entity["isAccessibleForFree"] = serde_json::json!(free);
    }
    if let Some(price) = &manifest.offers_price {
        let mut offer = serde_json::json!({
            "@type": "Offer",
            "price": price,
        });
        if let Some(currency) = &manifest.offers_price_currency {
            offer["priceCurrency"] = serde_json::json!(currency);
        }
        main_entity["offers"] = offer;
    }
    let mut web_page = serde_json::json!({
        "@context": "https://schema.org",
        "@type": "WebPage",
        "name": &manifest.site_name,
        "url": &manifest.canonical_url,
        "description": &locale.description,
        "inLanguage": &locale.lang,
    });
    if let Some(image) = &manifest.social_image_url {
        web_page["image"] = serde_json::json!(image);
        let mut primary_image = serde_json::json!({
            "@type": "ImageObject",
            "contentUrl": image,
        });
        if let Some(width) = manifest.social_image_width {
            primary_image["width"] = serde_json::json!(width);
        }
        if let Some(height) = manifest.social_image_height {
            primary_image["height"] = serde_json::json!(height);
        }
        if let Some(caption) = &manifest.social_image_alt {
            primary_image["caption"] = serde_json::json!(caption);
        }
        web_page["primaryImageOfPage"] = primary_image;
    }
    web_page["mainEntity"] = main_entity;
    let structured_data = serde_json::to_string_pretty(&web_page)?;

    let mut html = String::new();
    writeln!(html, "{GENERATED_BANNER}")?;
    writeln!(html, "<title>{}</title>", escape_html_text(&locale.title))?;
    writeln!(
        html,
        r#"<meta name="description" content="{}"/>"#,
        escape_html_attr(&locale.description)
    )?;
    writeln!(
        html,
        r#"<meta name="robots" content="{}"/>"#,
        escape_html_attr(&manifest.robots)
    )?;
    writeln!(
        html,
        r#"<meta name="theme-color" content="{}"/>"#,
        escape_html_attr(&manifest.theme_color)
    )?;
    writeln!(
        html,
        r#"<link rel="canonical" href="{}"/>"#,
        escape_html_attr(&manifest.canonical_url)
    )?;
    writeln!(
        html,
        r#"<link rel="icon" type="{}" href="{}"/>"#,
        icon_mime_type(&manifest.favicon_href),
        escape_html_attr(&manifest.favicon_href)
    )?;
    if let Some(apple_touch_icon) = &manifest.apple_touch_icon_href {
        writeln!(
            html,
            r#"<link rel="apple-touch-icon" href="{}"/>"#,
            escape_html_attr(apple_touch_icon)
        )?;
    }
    writeln!(html, r#"<meta property="og:type" content="website"/>"#)?;
    if let Some(og_locale) = &locale.og_locale {
        writeln!(
            html,
            r#"<meta property="og:locale" content="{}"/>"#,
            escape_html_attr(og_locale)
        )?;
    }
    writeln!(
        html,
        r#"<meta property="og:site_name" content="{}"/>"#,
        escape_html_attr(&manifest.site_name)
    )?;
    writeln!(
        html,
        r#"<meta property="og:title" content="{}"/>"#,
        escape_html_attr(&locale.title)
    )?;
    writeln!(
        html,
        r#"<meta property="og:description" content="{}"/>"#,
        escape_html_attr(&locale.description)
    )?;
    writeln!(
        html,
        r#"<meta property="og:url" content="{}"/>"#,
        escape_html_attr(&manifest.canonical_url)
    )?;
    if let Some(social_image_url) = &manifest.social_image_url {
        writeln!(
            html,
            r#"<meta property="og:image" content="{}"/>"#,
            escape_html_attr(social_image_url)
        )?;
        if let Some(social_image_alt) = &manifest.social_image_alt {
            writeln!(
                html,
                r#"<meta property="og:image:alt" content="{}"/>"#,
                escape_html_attr(social_image_alt)
            )?;
        }
        if let Some(social_image_type) = &manifest.social_image_type {
            writeln!(
                html,
                r#"<meta property="og:image:type" content="{}"/>"#,
                escape_html_attr(social_image_type)
            )?;
        }
        if let Some(social_image_width) = manifest.social_image_width {
            writeln!(
                html,
                r#"<meta property="og:image:width" content="{social_image_width}"/>"#
            )?;
        }
        if let Some(social_image_height) = manifest.social_image_height {
            writeln!(
                html,
                r#"<meta property="og:image:height" content="{social_image_height}"/>"#
            )?;
        }
    }
    if let Some(twitter_card) = &manifest.twitter_card {
        writeln!(
            html,
            r#"<meta name="twitter:card" content="{}"/>"#,
            escape_html_attr(twitter_card)
        )?;
        if let Some(twitter_site) = &manifest.twitter_site {
            writeln!(
                html,
                r#"<meta name="twitter:site" content="{}"/>"#,
                escape_html_attr(twitter_site)
            )?;
        }
        writeln!(
            html,
            r#"<meta name="twitter:title" content="{}"/>"#,
            escape_html_attr(&locale.title)
        )?;
        writeln!(
            html,
            r#"<meta name="twitter:description" content="{}"/>"#,
            escape_html_attr(&locale.description)
        )?;
        if let Some(social_image_url) = &manifest.social_image_url {
            writeln!(
                html,
                r#"<meta name="twitter:image" content="{}"/>"#,
                escape_html_attr(social_image_url)
            )?;
            if let Some(social_image_alt) = &manifest.social_image_alt {
                writeln!(
                    html,
                    r#"<meta name="twitter:image:alt" content="{}"/>"#,
                    escape_html_attr(social_image_alt)
                )?;
            }
        }
    }
    writeln!(html, r#"<script type="application/ld+json">"#)?;
    writeln!(html, "{structured_data}")?;
    writeln!(html, "</script>")?;
    Ok(html)
}

fn render_seo_body(locale: &SeoLocale) -> String {
    format!(
        concat!(
            "{banner}\n",
            "<h1 class=\"sr-only\">{name}</h1>\n",
            "<p class=\"sr-only\">{summary}</p>\n",
            "<noscript>\n",
            "  <div class=\"boot-noscript\" role=\"alert\">\n",
            "    <div class=\"boot-noscript-title\">{noscript_title}</div>\n",
            "    <div class=\"boot-noscript-message\">{noscript}</div>\n",
            "  </div>\n",
            "</noscript>\n"
        ),
        banner = GENERATED_BANNER,
        name = escape_html_text(&locale.accessible_name),
        summary = escape_html_text(&locale.accessible_summary),
        noscript_title = escape_html_text(&locale.noscript_title),
        noscript = escape_html_text(&locale.noscript),
    )
}

fn render_seo_sitemap(manifest: &SeoManifest) -> String {
    // The gameplay is a canvas: publish the screenshot here as well so image
    // discovery does not depend on executing the game or extracting its pixels.
    let image = manifest
        .social_image_url
        .as_ref()
        .map_or_else(String::new, |url| {
            format!(
                "    <image:image><image:loc>{}</image:loc></image:image>\n",
                escape_html_text(url)
            )
        });
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" ",
            "xmlns:image=\"http://www.google.com/schemas/sitemap-image/1.1\">\n",
            "  <url>\n",
            "    <loc>{}</loc>\n",
            "{}",
            "  </url>\n",
            "</urlset>\n"
        ),
        escape_html_text(&manifest.canonical_url),
        image,
    )
}

fn escape_html_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_html_attr(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::RegenSeo => regen_seo(&seo_paths()),
        Commands::GenSongs { check } => songs::gen_songs(check),
        Commands::GenDito { check } => dito::gen_dito(check),
        Commands::GenNoteskins { check } => noteskins::gen_noteskins(check),
        Commands::GenCredits { check } => credits::gen_credits(check),
        Commands::StageVideo { dest } => stage_video(dest),
        Commands::StageOpus { dest } => stage_opus(dest),
        Commands::CheckBoot { dest } => boot_check::check_boot(dest),
    }
}

/// The site's folder a post-build step `cmd` works on: `dest`, else
/// Trunk's `TRUNK_STAGING_DIR`.
fn staging_dir(dest: Option<PathBuf>, cmd: &str) -> Result<PathBuf> {
    match dest {
        Some(d) => Ok(d),
        None => std::env::var_os("TRUNK_STAGING_DIR")
            .map(PathBuf::from)
            .with_context(|| format!("{cmd} needs --dest or TRUNK_STAGING_DIR")),
    }
}

/// `stage-opus`: `opus-worker.js` and `opus-worker_bg.wasm` renamed to
/// `opus-worker-<hash>.js` and `opus-worker-<hash>_bg.wasm` (FNV-1a of
/// both), and `<meta name="ddi-opus-worker" content="opus-worker-<hash>">`
/// in index.html, which `crates/web/src/web/opus.rs` reads.
fn stage_opus(dest: Option<PathBuf>) -> Result<()> {
    let dest = staging_dir(dest, "stage-opus")?;
    let js = dest.join("opus-worker.js");
    let wasm = dest.join("opus-worker_bg.wasm");
    let (Ok(js_bytes), Ok(wasm_bytes)) = (fs::read(&js), fs::read(&wasm)) else {
        bail!(
            "stage-opus: {} has no opus-worker.js and opus-worker_bg.wasm (index.html's opus-worker link)",
            dest.display()
        );
    };
    let mut h = ddi_library::pack::Fnv1a64::new();
    h.write(&js_bytes);
    h.write(&wasm_bytes);
    let h = h.finish();
    let stem = format!("opus-worker-{h:016x}");
    fs::rename(&js, dest.join(format!("{stem}.js")))?;
    fs::rename(&wasm, dest.join(format!("{stem}_bg.wasm")))?;
    let index = dest.join("index.html");
    let html = fs::read_to_string(&index).context("reading the staged index.html")?;
    let meta = format!("<meta name=\"ddi-opus-worker\" content=\"{stem}\">");
    let Some(at) = html.find("</head>") else {
        bail!("stage-opus: index.html has no </head>");
    };
    fs::write(&index, format!("{}{meta}\n{}", &html[..at], &html[at..]))?;
    eprintln!("stage-opus: the Opus worker is {stem}");
    Ok(())
}

/// The module, its description and its licence, served from `video/`.
const VIDEO_FILES: [&str; 3] = ["ddivideo.wasm", "ddivideo.json", "ddivideo.LICENSE.txt"];

fn stage_video(dest: Option<PathBuf>) -> Result<()> {
    let dest = staging_dir(dest, "stage-video")?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let built = root.join("target/video");
    if std::env::var_os("DDI_NO_VIDEO").is_some_and(|v| !v.is_empty() && v != "0") {
        // The app works without it: movies only the module decodes show
        // the song background instead.
        eprintln!("stage-video: DDI_NO_VIDEO is set; building the site without the video decoder");
        return Ok(());
    }
    // `cargo clean` takes target/video with it, and a site built without
    // the module looks fine until a movie needs it; so build it here.
    if let Some(why) = video_out_of_date(&root, &built) {
        if cfg!(windows) {
            eprintln!(
                "stage-video: the video decoder is {why}, and video/build.sh does not run on Windows; building without it"
            );
            return Ok(());
        }
        eprintln!(
            "stage-video: the video decoder is {why}; building it (video/build.sh, a few minutes the first time)"
        );
        let status = std::process::Command::new("bash")
            .arg(root.join("video/build.sh"))
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .context("running video/build.sh")?;
        if !status.success() {
            bail!(
                "stage-video: video/build.sh failed ({status}); fix it, or set DDI_NO_VIDEO=1 to build the site without the video decoder"
            );
        }
    }
    // FFmpeg's source goes with the module (LGPL 2.1 section 4: offered
    // from the same place), as the release tarball the build checked.
    let tarball = credits::ffmpeg_tarball(&root)?;
    let source = built.join("downloads").join(&tarball);
    if !source.exists() {
        bail!(
            "stage-video: {} is missing; the module is never served without its source (run `just video`)",
            source.display()
        );
    }
    let out = dest.join("video");
    fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    for (from, name) in VIDEO_FILES
        .iter()
        .map(|name| (built.join(name), *name))
        .chain([(source.clone(), tarball.as_str())])
    {
        fs::copy(&from, out.join(name)).with_context(|| format!("copying {}", from.display()))?;
    }
    eprintln!(
        "stage-video: video decoder and its source copied to {}",
        out.display()
    );
    Ok(())
}

/// Why the module in `built` needs building, if it does: missing, or
/// older than a file it is built from (the build script then skips what
/// is current).
fn video_out_of_date(root: &Path, built: &Path) -> Option<&'static str> {
    let modified = |path: &Path| fs::metadata(path).and_then(|m| m.modified()).ok();
    let mut oldest = None;
    for name in VIDEO_FILES {
        let Some(time) = modified(&built.join(name)) else {
            return Some("not built");
        };
        oldest = Some(oldest.map_or(time, |t: std::time::SystemTime| t.min(time)));
    }
    let oldest = oldest?;
    let sources = [
        "video/build.sh",
        "video/FFMPEG.toml",
        "video/ddivideo.c",
        "video/packets.c",
    ];
    sources
        .iter()
        .filter_map(|source| modified(&root.join(source)))
        .any(|time| time > oldest)
        .then_some("older than its sources")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest() -> SeoManifest {
        let mut locales = BTreeMap::new();
        locales.insert(
            "en".to_string(),
            SeoLocale {
                lang: "en".to_string(),
                title: "Title & Co".to_string(),
                description: "A \"quoted\" description".to_string(),
                structured_data_description: "Structured description".to_string(),
                accessible_name: "Name <b>".to_string(),
                accessible_summary: "Summary".to_string(),
                noscript_title: "JavaScript is required.".to_string(),
                noscript: "Enable JavaScript.".to_string(),
                og_locale: Some("en_US".to_string()),
            },
        );
        SeoManifest {
            default_locale: "en".to_string(),
            site_name: "Dito Dito Infinito (Demo)".to_string(),
            alternate_name: None,
            theme_color: "#0a090d".to_string(),
            canonical_url: "https://example.test/app/".to_string(),
            favicon_href: "img/favicon.png".to_string(),
            apple_touch_icon_href: Some("img/favicon.png".to_string()),
            social_image_url: Some("https://example.test/app/img/og.png".to_string()),
            social_image_alt: Some("Alt text".to_string()),
            social_image_type: Some("image/png".to_string()),
            social_image_width: Some(1080),
            social_image_height: Some(1628),
            robots: "index,follow".to_string(),
            application_category: "GameApplication".to_string(),
            operating_system: "Web browser".to_string(),
            genres: vec!["Hidden object".to_string()],
            play_modes: vec!["SinglePlayer".to_string()],
            twitter_card: Some("summary".to_string()),
            twitter_site: None,
            is_accessible_for_free: Some(true),
            offers_price: None,
            offers_price_currency: None,
            locales,
        }
    }

    #[test]
    fn head_contains_one_theme_color_and_png_favicon() {
        let manifest = test_manifest();
        let (_, locale) = manifest.default_locale_entry().unwrap();
        let head = render_seo_head(&manifest, locale).unwrap();
        assert!(head.contains(r##"<meta name="theme-color" content="#0a090d"/>"##));
        assert!(!head.contains("prefers-color-scheme"));
        assert!(head.contains(r#"<link rel="icon" type="image/png" href="img/favicon.png"/>"#));
        assert!(head.contains(r#"<link rel="apple-touch-icon" href="img/favicon.png"/>"#));
    }

    #[test]
    fn the_page_colors_are_the_themes() {
        let colors = theme_colors(include_str!("../../crates/theme/src/infinito.toml")).unwrap();
        let css = render_theme_css(&colors).unwrap();
        assert!(
            css.contains(&format!("--ground: {};", colors["Ground"])),
            "{css}"
        );
        assert_eq!(css.matches("\n  --").count(), PAGE_COLORS.len(), "{css}");
    }

    #[test]
    fn icon_mime_type_is_inferred_from_extension() {
        assert_eq!(icon_mime_type("img/favicon.png"), "image/png");
        assert_eq!(icon_mime_type("favicon.svg"), "image/svg+xml");
        assert_eq!(icon_mime_type("favicon.ico"), "image/x-icon");
        assert_eq!(icon_mime_type("favicon.png?v=2"), "image/png");
    }

    #[test]
    fn head_escapes_html_in_text_and_attributes() {
        let manifest = test_manifest();
        let (_, locale) = manifest.default_locale_entry().unwrap();
        let head = render_seo_head(&manifest, locale).unwrap();
        assert!(head.contains("<title>Title &amp; Co</title>"));
        assert!(head.contains(r#"content="A &quot;quoted&quot; description""#));
    }

    #[test]
    fn head_structured_data_has_video_game_entity() {
        let manifest = test_manifest();
        let (_, locale) = manifest.default_locale_entry().unwrap();
        let head = render_seo_head(&manifest, locale).unwrap();
        assert!(head.contains(r#""@type": "VideoGame""#));
        assert!(head.contains(r#""playMode""#));
        assert!(head.contains(r#""SinglePlayer""#));
        assert!(head.contains(r#""isAccessibleForFree": true"#));
        let json = head
            .split("<script type=\"application/ld+json\">\n")
            .nth(1)
            .unwrap();
        let data: serde_json::Value =
            serde_json::from_str(json.split("</script>").next().unwrap()).unwrap();
        assert_eq!(
            data["mainEntity"]["screenshot"],
            "https://example.test/app/img/og.png"
        );
        assert_eq!(
            data["primaryImageOfPage"]["contentUrl"],
            data["mainEntity"]["image"]
        );
        assert_eq!(data["primaryImageOfPage"]["width"], 1080);
        assert_eq!(data["primaryImageOfPage"]["height"], 1628);
    }

    #[test]
    fn head_omits_optional_tags_when_unset() {
        let mut manifest = test_manifest();
        manifest.twitter_card = None;
        manifest.apple_touch_icon_href = None;
        manifest.alternate_name = None;
        manifest.play_modes.clear();
        let (_, locale) = manifest.default_locale_entry().unwrap();
        let head = render_seo_head(&manifest, locale).unwrap();
        assert!(!head.contains("twitter:"));
        assert!(!head.contains("apple-touch-icon"));
        assert!(!head.contains("alternateName"));
        assert!(!head.contains("playMode"));
    }

    #[test]
    fn head_omits_all_image_tags_without_social_image() {
        let mut manifest = test_manifest();
        manifest.social_image_url = None;
        manifest.social_image_alt = None;
        manifest.social_image_type = None;
        manifest.social_image_width = None;
        manifest.social_image_height = None;
        let (_, locale) = manifest.default_locale_entry().unwrap();
        let head = render_seo_head(&manifest, locale).unwrap();
        assert!(!head.contains("og:image"));
        assert!(!head.contains("twitter:image"));
        assert!(!head.contains(r#""image""#));
        assert!(!head.contains("primaryImageOfPage"));
        assert!(!head.contains("screenshot"));
        // The twitter card itself survives without an image.
        assert!(head.contains(r#"<meta name="twitter:card" content="summary"/>"#));
    }

    #[test]
    fn body_uses_sr_only_and_boot_noscript_markup() {
        let manifest = test_manifest();
        let (_, locale) = manifest.default_locale_entry().unwrap();
        let body = render_seo_body(locale);
        assert!(body.contains(r#"<h1 class="sr-only">Name &lt;b&gt;</h1>"#));
        assert!(body.contains(r#"<div class="boot-noscript" role="alert">"#));
        assert!(body.contains(r#"<div class="boot-noscript-title">JavaScript is required.</div>"#));
        assert!(body.contains(r#"<div class="boot-noscript-message">Enable JavaScript.</div>"#));
    }

    #[test]
    fn sitemap_contains_canonical_url() {
        let mut manifest = test_manifest();
        manifest.social_image_url = Some("https://example.test/shot.png?a=1&b=2".into());
        let sitemap = render_seo_sitemap(&manifest);
        assert!(sitemap.contains("<loc>https://example.test/app/</loc>"));
        assert!(
            sitemap.contains("<image:loc>https://example.test/shot.png?a=1&amp;b=2</image:loc>")
        );
        manifest.social_image_url = None;
        assert!(!render_seo_sitemap(&manifest).contains("<image:image>"));
    }

    #[test]
    fn real_manifest_parses_and_renders() {
        let paths = seo_paths();
        let manifest_text = fs::read_to_string(&paths.metadata).unwrap();
        let manifest: SeoManifest = toml::from_str(&manifest_text).unwrap();
        let (_, locale) = manifest.default_locale_entry().unwrap();
        let head = render_seo_head(&manifest, locale).unwrap();
        assert!(head.contains("<title>"));
        assert!(head.contains("Dito Dito Infinito"));
        assert_eq!(
            manifest.twitter_card.as_deref(),
            Some("summary_large_image")
        );
        let url = manifest.social_image_url.as_deref().unwrap();
        assert_eq!(url, format!("{}images/readme.png", manifest.canonical_url));
        assert!(head.contains(&format!(r#"property="og:image" content="{url}""#)));
        assert!(head.contains(&format!(r#"name="twitter:image" content="{url}""#)));
        // The metadata must describe the actual shipped image, not just a
        // plausible URL or dimensions from an earlier screenshot.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let png = fs::read(root.join("docs/screenshots/readme.png")).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            manifest.social_image_width,
            Some(u32::from_be_bytes(png[16..20].try_into().unwrap()))
        );
        assert_eq!(
            manifest.social_image_height,
            Some(u32::from_be_bytes(png[20..24].try_into().unwrap()))
        );
        let index = fs::read_to_string(root.join("site/index.html")).unwrap();
        assert!(index.contains(r#"<link data-trunk rel="copy-file" href="../docs/screenshots/readme.png" data-target-path="images" />"#));
    }
}
