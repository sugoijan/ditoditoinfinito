//! Render an external candidate font beside the installed title-font policy.
//! All outputs stay outside Git; no font installation or source-work execution.
use ddi_danoni_export::{
    artwork::{PORTABLE_JAPANESE, PORTABLE_LATIN, Role, TitleRenderer, portable_fonts},
    assets::sha256,
};
use image::{Rgb, RgbImage, imageops};
use resvg::{tiny_skia, usvg};
use serde_json::json;
use std::{error::Error, fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TITLES: &[(&str, &str)] = &[
    ("Crow", "ZUN"),
    ("メイドと血の懐中時計", "上海アリス幻樂団 / ZUN"),
    ("上海紅茶館 ～ Chinese Tea", "上海アリス幻樂団 / ZUN"),
    ("風神少女 ～ The Crow's Newspaper", "上海アリス幻樂団 / ZUN"),
    ("灼冬赫夜", "No Name / Remake"),
    ("ふしぎなひかり", "9A / 9B"),
    ("桃源郷", "Normal / Hard"),
    ("北風のメロディー", "2004 紅白"),
    ("ころっけわっしょい", "2004 紅白"),
    ("運 進 週 選 過 達 通 連 遠", "Japanese glyph forms"),
    (
        "が / か\u{3099} / ガ / カ\u{3099}",
        "Composed / decomposed kana",
    ),
    (
        "“Title” — ★☆ ♪ → ～ / 0123456789",
        "Punctuation and symbols",
    ),
];
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn options(font: &[u8], fallback: Option<&[u8]>) -> Result<usvg::Options<'static>> {
    let mut opts = usvg::Options::default();
    opts.fontdb_mut().load_font_data(font.to_vec());
    opts.font_family = opts.fontdb.faces().next().ok_or("No font face")?.families[0]
        .0
        .clone();
    if let Some(bytes) = fallback {
        opts.fontdb_mut().load_font_data(bytes.to_vec());
    }
    opts.languages = vec!["ja".into(), "en".into()];
    opts.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: Box::new(|_, _, _| None),
        resolve_string: Box::new(|_, _| None),
    };
    Ok(opts)
}
fn raster(svg: &str, options: &usvg::Options<'_>) -> Result<RgbImage> {
    fn check(group: &usvg::Group, width: f32, height: f32) -> Result<()> {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => check(g, width, height)?,
                usvg::Node::Text(t) => {
                    if t.layouted()
                        .iter()
                        .flat_map(|s| &s.positioned_glyphs)
                        .any(|g| g.id.0 == 0)
                    {
                        return Err("Missing specimen glyph".into());
                    }
                    let b = t.abs_bounding_box();
                    if b.x() < 0.0 || b.y() < 0.0 || b.right() > width || b.bottom() > height {
                        return Err("Clipped specimen text".into());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    let tree = usvg::Tree::from_str(svg, options)?;
    let size = tree.size().to_int_size();
    check(tree.root(), size.width() as f32, size.height() as f32)?;
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).ok_or("Pixmap size")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(image::load_from_memory(&pixmap.encode_png()?)?.to_rgb8())
}
fn label(text: &str) -> Result<RgbImage> {
    raster(
        &format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='640' height='48'><rect width='640' height='48' fill='#20242c'/><text x='16' y='31' font-size='20' fill='white'>{}</text></svg>",
            escape(text)
        ),
        &options(PORTABLE_LATIN, None)?,
    )
}
fn pair(
    left: &RgbImage,
    right: &RgbImage,
    left_label: &str,
    right_label: &str,
) -> Result<RgbImage> {
    let mut sheet = RgbImage::from_pixel(1312, left.height() + 64, Rgb([32, 36, 44]));
    imageops::replace(&mut sheet, &label(left_label)?, 8, 0);
    imageops::replace(&mut sheet, &label(right_label)?, 664, 0);
    imageops::replace(&mut sheet, left, 8, 48);
    imageops::replace(&mut sheet, right, 664, 48);
    Ok(sheet)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: compare_title_fonts CANDIDATE_FONT NEW_EXTERNAL_OUTPUT".into());
    }
    let font = fs::read(&args[0])?;
    let candidate = TitleRenderer::new(&font)?;
    let baseline = TitleRenderer::portable()?;
    let out = Path::new(&args[1]);
    let parent = out.parent().ok_or("Output parent")?.canonicalize()?;
    if parent.ancestors().any(|p| p.join(".git").exists()) {
        return Err("Comparison outputs must stay outside Git checkouts".into());
    }
    fs::create_dir(out)?;
    let candidate_options = options(&font, None)?;
    let family = candidate_options.font_family.clone();
    let mut records = Vec::new();
    let mut overview = RgbImage::from_pixel(1312, 4 * 264, Rgb([32, 36, 44]));
    let mut html = format!(
        "<!doctype html><meta charset='utf-8'><title>Title font comparison</title><style>body{{background:#181b21;color:#eee;font:16px system-ui;margin:24px}}img{{display:block;max-width:100%;height:auto;margin:16px 0}}a{{color:#9dcfff}}</style><h1>Iosevka / Noto and {}</h1><p>Identical production banner layout; left: current packaged fonts, right: candidate. Images contain their own text and need no installed fonts.</p>",
        escape(&family)
    );
    for (i, (title, credit)) in TITLES.iter().enumerate() {
        let (_, a, before) = baseline.card(title, credit, Role::Banner)?;
        let (_, b, after) = candidate.card(title, credit, Role::Banner)?;
        let sheet = pair(
            &image::load_from_memory(&a)?.to_rgb8(),
            &image::load_from_memory(&b)?.to_rgb8(),
            "Current: Iosevka + Noto CJK JP",
            &family,
        )?;
        let name = format!("banner-{i:02}.png");
        sheet.save(out.join(&name))?;
        if i < 4 {
            imageops::replace(&mut overview, &sheet, 0, (i * 264) as i64);
        }
        html.push_str(&format!(
            "<h2>{}</h2><a href='{name}'><img src='{name}' width='1312' alt='{}'></a>",
            escape(title),
            escape(title)
        ));
        records.push(
            json!({"title":title,"credit":credit,"baseline":before,"candidate":after,"image":name}),
        );
    }
    overview.save(out.join("overview.png"))?;
    let specimen = |opts: &usvg::Options<'_>| -> Result<RgbImage> {
        let mut text = String::new();
        for (row, size) in [16, 20, 28].into_iter().enumerate() {
            for (line, sample) in [
                "上海紅茶館 ～ Chinese Tea",
                "メイドと血の懐中時計 / Crow",
                "“Title” — ★☆ ♪ → 0123456789",
            ]
            .iter()
            .enumerate()
            {
                text.push_str(&format!(
                    "<text x='16' y='{}' font-size='{size}'>{}</text>",
                    36 + row * 132 + line * 37,
                    escape(sample)
                ));
            }
        }
        raster(
            &format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='640' height='408'><rect width='640' height='408' fill='black'/><g fill='white'>{text}</g></svg>"
            ),
            opts,
        )
    };
    pair(
        &specimen(&options(PORTABLE_LATIN, Some(PORTABLE_JAPANESE))?)?,
        &specimen(&candidate_options)?,
        "Current / 16, 20, 28 px",
        &format!("{family} / 16, 20, 28 px"),
    )?
    .save(out.join("small-text.png"))?;
    html.push_str("<h2>Small text: 16, 20, 28 px</h2><img src='small-text.png' width='1312' alt='Small text comparison'><p>Same usvg/resvg shaping and rasterization as banners; this is not a test of the desktop/browser UI text renderer.</p>");
    fs::write(out.join("index.html"), html)?;
    fs::write(
        out.join("comparison.json"),
        serde_json::to_vec_pretty(
            &json!({"schema":"ddi-font-comparison/1","candidate":{"family":family,"sha256":sha256(&font),"bytes":font.len()},"baseline":portable_fonts(),"records":records,"systemFonts":false,"productionDefaultChanged":false}),
        )?,
    )?;
    println!("{}", out.join("index.html").display());
    Ok(())
}
